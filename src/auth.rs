use crate::{
    db::Database,
    error::{ApiError, Result},
};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use axum::http::StatusCode;
use rusqlite::{OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;
use uuid::Uuid;
use zeroize::Zeroize;

const IDLE_SECONDS: i64 = 7 * 86400;
const MAX_SECONDS: i64 = 30 * 86400;

#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Login {
    pub username: String,
    pub password: String,
    pub device: String,
}
impl Drop for Login {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Redeem {
    pub invite_code: String,
    pub username: String,
    pub display_name: String,
    pub password: String,
    pub device: String,
}
impl Drop for Redeem {
    fn drop(&mut self) {
        self.password.zeroize();
        self.invite_code.zeroize();
    }
}
#[derive(Deserialize, ToSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Recover {
    pub recovery_code: String,
    pub password: String,
    pub device: String,
}
impl Drop for Recover {
    fn drop(&mut self) {
        self.password.zeroize();
        self.recovery_code.zeroize();
    }
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SignedIn {
    pub token: String,
    pub account_id: String,
    pub username: String,
    pub display_name: String,
}
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSession {
    pub id: String,
    pub device: String,
    pub created_at: String,
    pub last_used_at: String,
    pub expires_at: String,
    pub current: bool,
}
pub(crate) struct Identity {
    pub account_id: String,
    pub session_id: String,
}

pub fn now() -> i64 {
    chrono::Utc::now().timestamp()
}
pub fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
pub fn random_secret() -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|_| ApiError::internal())?;
    let result = bytes.iter().map(|b| format!("{b:02x}")).collect();
    bytes.zeroize();
    Ok(result)
}
pub fn password_hash(password: &str) -> Result<String> {
    let mut salt = [0u8; 16];
    getrandom::fill(&mut salt).map_err(|_| ApiError::internal())?;
    let salt = SaltString::encode_b64(&salt).map_err(|_| ApiError::internal())?;
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| ApiError::internal())
}
pub(crate) fn verify(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|hash| {
        Argon2::default()
            .verify_password(password.as_bytes(), &hash)
            .is_ok()
    })
}
fn text(value: &str, min: usize, max: usize) -> bool {
    let len = value.chars().count();
    len >= min && len <= max && value.trim() == value && !value.chars().any(char::is_control)
}
fn username(value: &str) -> Result<String> {
    if !text(value, 3, 64)
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        return Err(ApiError::invalid());
    }
    Ok(value.to_ascii_lowercase())
}
fn credentials(password: &str, device: &str) -> Result<()> {
    if !(12..=128).contains(&password.chars().count())
        || password.len() > 512
        || password.chars().any(char::is_control)
        || !text(device, 1, 100)
    {
        return Err(ApiError::invalid());
    }
    Ok(())
}
fn secret(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(ApiError::invalid());
    }
    Ok(())
}
fn timestamp(seconds: i64) -> String {
    chrono::DateTime::from_timestamp(seconds, 0)
        .unwrap()
        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

impl Database {
    /// Codes are shown only by an explicit operator CLI command, never in request logs.
    pub fn issue_invite(&self, recovery_username: Option<&str>) -> Result<String> {
        self.issue_invite_with_gm(recovery_username, false)
    }
    pub fn issue_invite_with_gm(
        &self,
        recovery_username: Option<&str>,
        grant_gm: bool,
    ) -> Result<String> {
        if grant_gm && recovery_username.is_some() {
            return Err(ApiError::invalid());
        }
        let code = random_secret()?;
        self.with(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let account: Option<String> = if let Some(name) = recovery_username {
                Some(tx.query_row("SELECT id FROM accounts WHERE username=?1 AND disabled=0", [username(name)?], |r| r.get(0))
                    .optional()?.ok_or(ApiError(StatusCode::NOT_FOUND, "Account not found."))?)
            } else { None };
            if let Some(id) = &account {
                tx.execute("UPDATE invites SET used_at=?1 WHERE recovery_account_id=?2 AND used_at IS NULL", params![now(),id])?;
            }
            tx.execute("DELETE FROM invites WHERE expires_at<?1", [now() - MAX_SECONDS])?;
            tx.execute("INSERT INTO invites(code_hash,recovery_account_id,created_at,expires_at,grant_gm) VALUES(?1,?2,?3,?4,?5)",
                params![digest(&code), account, now(), now() + if recovery_username.is_some() {3600} else {86400},grant_gm])?;
            tx.commit()?;
            Ok(code.clone())
        })
    }
    /// Persistent, bounded-by-window buckets: restarts do not reset attempt limits.
    pub fn attempt(&self, ip: &str, principal: &str, purpose: &str) -> Result<()> {
        self.with(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let time = now();
            tx.execute("DELETE FROM rate_buckets WHERE reset_at<=?1", [time])?;
            let mut buckets: i64 = tx.query_row("SELECT count(*) FROM rate_buckets", [], |r| r.get(0))?;
            for (key, limit) in [(format!("ip:{ip}"), 30), (format!("{purpose}:{principal}"), 5)] {
                let key = digest(&key);
                let attempts: Option<i64> = tx.query_row("SELECT attempts FROM rate_buckets WHERE key_hash=?1", [&key], |r| r.get(0)).optional()?;
                if attempts.is_none() {
                    if buckets >= 10000 { tx.commit()?; return Err(ApiError::limited()); }
                    buckets += 1;
                }
                // Commit the IP counter even when a principal is already blocked.
                if attempts.unwrap_or(0) >= limit { tx.commit()?; return Err(ApiError::limited()); }
                tx.execute("INSERT INTO rate_buckets VALUES(?1,1,?2) ON CONFLICT(key_hash) DO UPDATE SET attempts=attempts+1", params![key, time+900])?;
            }
            tx.commit()?;
            Ok(())
        })
    }
    pub fn login(&self, request: &Login, ip: &str, dummy_hash: &str) -> Result<SignedIn> {
        credentials(&request.password, &request.device)?;
        let name = username(&request.username)?;
        self.attempt(ip, &name, "login")?;
        let hash: Option<String> = self.with(|conn| {
            Ok(conn
                .query_row(
                    "SELECT password_hash FROM accounts WHERE username=?1 AND disabled=0",
                    [&name],
                    |r| r.get(0),
                )
                .optional()?)
        })?;
        let valid = verify(&request.password, hash.as_deref().unwrap_or(dummy_hash));
        if !valid || hash.is_none() {
            return Err(ApiError::unauthorized());
        }
        self.with(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let account: Option<(String, String)> = tx.query_row("SELECT id,display_name FROM accounts WHERE username=?1 AND password_hash=?2 AND disabled=0", params![name,hash], |r| Ok((r.get(0)?,r.get(1)?))).optional()?;
            let (id, display) = account.ok_or_else(ApiError::unauthorized)?;
            let result = issue_session(&tx, &id, &name, &display, &request.device)?;
            tx.commit()?;
            Ok(result)
        })
    }
    pub fn redeem(&self, request: &Redeem, ip: &str) -> Result<SignedIn> {
        credentials(&request.password, &request.device)?;
        let name = username(&request.username)?;
        if !text(&request.display_name, 1, 100) {
            return Err(ApiError::invalid());
        }
        secret(&request.invite_code)?;
        self.attempt(ip, &digest(&request.invite_code), "redeem")?;
        let hash = password_hash(&request.password)?;
        self.with(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let valid: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM invites WHERE code_hash=?1 AND recovery_account_id IS NULL AND used_at IS NULL AND expires_at>?2)", params![digest(&request.invite_code),now()], |r| r.get(0))?;
            let table_invite = crate::tables::available_invite(&tx,&digest(&request.invite_code))?;
            if !valid && !table_invite && !crate::campaigns::available_invite(&tx,&digest(&request.invite_code))? { return Err(ApiError::gone()); }
            let exists: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM accounts WHERE username=?1)", [&name], |r| r.get(0))?;
            if exists { return Err(ApiError(StatusCode::CONFLICT, "Username unavailable.")); }
            let id = Uuid::new_v4().to_string();
            tx.execute("INSERT INTO accounts(id,username,display_name,password_hash,created_at,gm_enabled) VALUES(?1,?2,?3,?4,?5,coalesce((SELECT grant_gm FROM invites WHERE code_hash=?6),0))", params![id,name,request.display_name,hash,now(),digest(&request.invite_code)])?;
            if valid {
                tx.execute("UPDATE invites SET used_at=?1 WHERE code_hash=?2", params![now(),digest(&request.invite_code)])?;
            } else if table_invite {
                crate::tables::accept_invite(&tx,&digest(&request.invite_code),&id)?;
            } else {
                crate::campaigns::accept_invite(&tx,&digest(&request.invite_code),&id)?;
            }
            let result = issue_session(&tx, &id, &name, &request.display_name, &request.device)?;
            tx.commit()?;
            Ok(result)
        })
    }
    pub fn recover(&self, request: &Recover, ip: &str) -> Result<SignedIn> {
        credentials(&request.password, &request.device)?;
        secret(&request.recovery_code)?;
        self.attempt(ip, &digest(&request.recovery_code), "recover")?;
        let hash = password_hash(&request.password)?;
        self.with(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let account: Option<(String,String,String)> = tx.query_row("SELECT a.id,a.username,a.display_name FROM invites i JOIN accounts a ON a.id=i.recovery_account_id WHERE i.code_hash=?1 AND i.used_at IS NULL AND i.expires_at>?2 AND a.disabled=0", params![digest(&request.recovery_code),now()], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
            let (id, name, display) = account.ok_or_else(ApiError::gone)?;
            tx.execute("UPDATE accounts SET password_hash=?1 WHERE id=?2", params![hash,id])?;
            tx.execute("UPDATE invites SET used_at=?1 WHERE recovery_account_id=?2 AND used_at IS NULL", params![now(),id])?;
            tx.execute("UPDATE sessions SET revoked_at=?1 WHERE account_id=?2 AND revoked_at IS NULL", params![now(),id])?;
            let result = issue_session(&tx, &id, &name, &display, &request.device)?;
            tx.commit()?;
            Ok(result)
        })
    }
    pub(crate) fn identity(&self, token: &str) -> Result<Identity> {
        if secret(token).is_err() {
            return Err(ApiError::unauthorized());
        }
        self.with(|conn| {
            let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let time = now();
            let identity = tx.query_row("SELECT s.account_id,s.id FROM sessions s JOIN accounts a ON a.id=s.account_id WHERE s.token_hash=?1 AND s.revoked_at IS NULL AND s.expires_at>?2 AND s.absolute_expires_at>?2 AND a.disabled=0", params![digest(token),time], |r| Ok(Identity { account_id:r.get(0)?,session_id:r.get(1)? })).optional()?.ok_or_else(ApiError::unauthorized)?;
            tx.execute("UPDATE sessions SET last_used_at=?1,expires_at=min(?2,absolute_expires_at) WHERE id=?3", params![time,time+IDLE_SECONDS,identity.session_id])?;
            tx.commit()?;
            Ok(identity)
        })
    }
    pub fn sessions(&self, token: &str) -> Result<Vec<DeviceSession>> {
        let identity = self.identity(token)?;
        self.with(|conn| {
            let mut statement = conn.prepare("SELECT id,device,created_at,last_used_at,expires_at FROM sessions WHERE account_id=?1 AND revoked_at IS NULL AND expires_at>?2 AND absolute_expires_at>?2 ORDER BY created_at,id")?;
            let result = statement.query_map(params![identity.account_id,now()], |row| {
                let id: String = row.get(0)?;
                Ok(DeviceSession { current:id == identity.session_id,id,device:row.get(1)?,created_at:timestamp(row.get(2)?),last_used_at:timestamp(row.get(3)?),expires_at:timestamp(row.get(4)?) })
            })?.collect::<std::result::Result<Vec<_>,_>>()?;
            Ok(result)
        })
    }
    pub fn revoke(&self, token: &str, session_id: Option<&str>, all: bool) -> Result<()> {
        let identity = self.identity(token)?;
        if let Some(id) = session_id {
            Uuid::parse_str(id).map_err(|_| ApiError::invalid())?;
        }
        self.with(|conn| {
            let affected = if all {
                conn.execute("UPDATE sessions SET revoked_at=?1 WHERE account_id=?2 AND revoked_at IS NULL", params![now(),identity.account_id])?
            } else {
                conn.execute("UPDATE sessions SET revoked_at=?1 WHERE account_id=?2 AND id=?3 AND revoked_at IS NULL", params![now(),identity.account_id,session_id.unwrap_or(&identity.session_id)])?
            };
            if affected == 0 { return Err(ApiError(StatusCode::NOT_FOUND, "Session not found.")); }
            Ok(())
        })
    }
}

fn issue_session(
    conn: &rusqlite::Connection,
    id: &str,
    username: &str,
    display: &str,
    device: &str,
) -> Result<SignedIn> {
    let time = now();
    conn.execute("DELETE FROM sessions WHERE absolute_expires_at<=?1 OR expires_at<=?1 OR revoked_at IS NOT NULL", [time])?;
    let count: i64 = conn.query_row(
        "SELECT count(*) FROM sessions WHERE account_id=?1",
        [id],
        |r| r.get(0),
    )?;
    if count >= 20 {
        return Err(ApiError::limited());
    }
    let token = random_secret()?;
    conn.execute("INSERT INTO sessions(id,token_hash,account_id,device,created_at,last_used_at,expires_at,absolute_expires_at) VALUES(?1,?2,?3,?4,?5,?5,?6,?7)", params![Uuid::new_v4().to_string(),digest(&token),id,device,time,time+IDLE_SECONDS,time+MAX_SECONDS])?;
    Ok(SignedIn {
        token,
        account_id: id.into(),
        username: username.into(),
        display_name: display.into(),
    })
}
