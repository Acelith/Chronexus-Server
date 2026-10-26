//! Bounded, content-free change notifications (server-sent events).
//!
//! A frame never names an item, kind, author or cursor: it only tells an authenticated
//! member that a re-read of `/sync` is worthwhile. Private edits notify their author
//! only, so other members cannot infer activity. Polling `/sync` remains the fallback.
use crate::{
    AppState,
    error::{ApiError, Result},
};
use axum::{
    body::{Body, Bytes},
    http::StatusCode,
    response::Response,
};
use futures_core::Stream;
use std::{
    collections::HashMap,
    convert::Infallible,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::{Duration, Instant},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, broadcast::error::RecvError, mpsc};

const GLOBAL_STREAMS: usize = 64;
const ACCOUNT_STREAMS: usize = 3;
const KEEPALIVE: Duration = Duration::from_secs(25);
/// Streams close after this long; clients reconnect and re-poll.
const LIFETIME: Duration = Duration::from_secs(10 * 60);

pub(crate) struct Streams {
    total: Arc<Semaphore>,
    accounts: Mutex<HashMap<String, usize>>,
}
impl Streams {
    pub(crate) fn new() -> Self {
        Self {
            total: Arc::new(Semaphore::new(GLOBAL_STREAMS)),
            accounts: Mutex::new(HashMap::new()),
        }
    }
    fn open(self: &Arc<Self>, account: &str) -> Result<Guard> {
        let busy = || {
            ApiError(
                StatusCode::TOO_MANY_REQUESTS,
                "Too many open notification streams. Poll instead.",
            )
        };
        let permit = self.total.clone().try_acquire_owned().map_err(|_| busy())?;
        let mut accounts = self.accounts.lock().map_err(|_| ApiError::internal())?;
        let count = accounts.entry(account.to_string()).or_default();
        if *count >= ACCOUNT_STREAMS {
            return Err(busy());
        }
        *count += 1;
        Ok(Guard {
            streams: self.clone(),
            account: account.to_string(),
            _permit: permit,
        })
    }
}
struct Guard {
    streams: Arc<Streams>,
    account: String,
    _permit: OwnedSemaphorePermit,
}
impl Drop for Guard {
    fn drop(&mut self) {
        if let Ok(mut accounts) = self.streams.accounts.lock()
            && let Some(count) = accounts.get_mut(&self.account)
        {
            *count -= 1;
            if *count == 0 {
                accounts.remove(&self.account);
            }
        }
    }
}
struct Frames(mpsc::Receiver<Bytes>);
impl Stream for Frames {
    type Item = std::result::Result<Bytes, Infallible>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.0.poll_recv(cx).map(|frame| frame.map(Ok))
    }
}
fn event(name: &'static str) -> Bytes {
    Bytes::from(format!("event: {name}\ndata: {{}}\n\n"))
}

/// Opens a notification stream for an already authorized member.
pub(crate) async fn open(state: AppState, token: String, campaign: String) -> Result<Response> {
    let account = {
        let token = token.clone();
        state.blocking(move |db| db.account_for(&token)).await?
    };
    let guard = state.streams.open(&account)?;
    // Subscribe before replying so nothing committed after `ready` can be missed.
    let mut signals = state.database.subscribe();
    let (sender, receiver) = mpsc::channel::<Bytes>(4);
    tokio::spawn(async move {
        let _guard = guard;
        let started = Instant::now();
        if sender.send(event("ready")).await.is_err() {
            return;
        }
        let mut tick = tokio::time::interval(KEEPALIVE);
        tick.tick().await;
        loop {
            tokio::select! {
                _ = sender.closed() => return,
                signal = signals.recv() => match signal {
                    Ok(signal) => {
                        if signal.campaign == campaign && (signal.public || signal.author == account) {
                            // A full queue already holds a pending hint; coalesce.
                            let _ = sender.try_send(event("changed"));
                        }
                    }
                    Err(RecvError::Lagged(_)) => {
                        let _ = sender.try_send(event("changed"));
                    }
                    Err(RecvError::Closed) => return,
                },
                _ = tick.tick() => {
                    if started.elapsed() >= LIFETIME {
                        let _ = sender.send(event("reconnect")).await;
                        return;
                    }
                    let (token, campaign) = (token.clone(), campaign.clone());
                    match state.blocking(move |db| db.authorize_content(&token, &campaign, false)).await {
                        Ok(()) => {}
                        // Saturated workers must not drop a healthy stream.
                        Err(ApiError(StatusCode::SERVICE_UNAVAILABLE, _)) => {}
                        Err(_) => {
                            let _ = sender.send(event("closed")).await;
                            return;
                        }
                    }
                    if sender.send(Bytes::from_static(b": keepalive\n\n")).await.is_err() {
                        return;
                    }
                }
            }
        }
    });
    Response::builder()
        .header("content-type", "text/event-stream")
        .header("x-accel-buffering", "no")
        .body(Body::from_stream(Frames(receiver)))
        .map_err(|_| ApiError::internal())
}
