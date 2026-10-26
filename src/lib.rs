pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod routes;

use crate::{
    config::Config,
    db::Database,
    error::{ApiError, Result},
};
use std::sync::Arc;
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub database: Arc<Database>,
    pub(crate) workers: Arc<Semaphore>,
    pub(crate) dummy_hash: Arc<String>,
}
impl AppState {
    pub fn new(config: Config) -> std::result::Result<Self, String> {
        let database = Database::open(&config.database)?;
        let dummy_hash =
            auth::password_hash(&auth::random_secret().map_err(|_| "Random source unavailable.")?)
                .map_err(|_| "Cannot initialize password verification.")?;
        Ok(Self {
            config: Arc::new(config),
            database: Arc::new(database),
            workers: Arc::new(Semaphore::new(4)),
            dummy_hash: Arc::new(dummy_hash),
        })
    }
    pub async fn blocking<T, F>(&self, operation: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&Database) -> Result<T> + Send + 'static,
    {
        let permit = self.workers.clone().try_acquire_owned().map_err(|_| {
            ApiError(
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                "Server busy. Try again later.",
            )
        })?;
        let database = self.database.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            operation(&database)
        })
        .await
        .map_err(|_| ApiError::internal())?
    }
}
