use crate::error::{ApiError, Result};
use rusqlite::{Connection, TransactionBehavior};
use std::{
    fs,
    path::{Component, Path},
    sync::Mutex,
    time::Duration,
};

pub struct Database(pub(crate) Mutex<Connection>);
const SERVER_ID: i64 = 0x43584e53;

fn inspect_identity(conn: &Connection) -> std::result::Result<u32, String> {
    let version: u32 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(|_| "Cannot read schema version.")?;
    let application_id: i64 = conn
        .pragma_query_value(None, "application_id", |r| r.get(0))
        .map_err(|_| "Cannot read database identity.")?;
    if version > 4 {
        return Err("Database schema is newer than this server; use a compatible binary.".into());
    }
    if application_id != SERVER_ID && !(application_id == 0 && version < 3) {
        return Err(
            "Database belongs to another application or has no valid server identity.".into(),
        );
    }
    if version > 0 && application_id == 0 {
        for sql in [
            "SELECT id,username,display_name,password_hash,created_at,disabled FROM accounts LIMIT 0",
            "SELECT id,token_hash,account_id,device,expires_at,absolute_expires_at,revoked_at FROM sessions LIMIT 0",
            "SELECT code_hash,recovery_account_id,expires_at,used_at FROM invites LIMIT 0",
        ] {
            conn.prepare(sql).map_err(
                |_| "Refusing to migrate a database that is not a Chronexus server archive.",
            )?;
        }
    }
    if version == 0 {
        let tables: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
                [],
                |r| r.get(0),
            )
            .map_err(|_| "Cannot inspect unversioned database.")?;
        if tables != 0 {
            return Err("Refusing to migrate an unrecognized existing database.".into());
        }
    }
    Ok(version)
}

impl Database {
    pub fn open(path: &Path) -> std::result::Result<Self, String> {
        // Reject symlinks in every existing component, including the database file.
        let absolute = std::path::absolute(path).map_err(|_| "Invalid database path.")?;
        let mut current = std::path::PathBuf::new();
        for component in absolute.components() {
            if matches!(component, Component::ParentDir) {
                return Err("Database path must not contain parent traversal.".into());
            }
            current.push(component);
            match fs::symlink_metadata(&current) {
                Ok(meta) if meta.file_type().is_symlink() => {
                    return Err("Database path must not traverse symbolic links.".into());
                }
                Ok(_) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err("Cannot inspect database path.".into()),
            }
        }
        if let Some(parent) = absolute.parent() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                let mut builder = fs::DirBuilder::new();
                builder
                    .recursive(true)
                    .mode(0o700)
                    .create(parent)
                    .map_err(|_| "Cannot create data directory.")?;
            }
            #[cfg(not(unix))]
            fs::create_dir_all(parent).map_err(|_| "Cannot create data directory.")?;
        }
        let mut options = fs::OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options
            .open(&absolute)
            .map_err(|_| "Cannot open database file.")?;
        for suffix in ["-wal", "-shm"] {
            let mut companion = absolute.as_os_str().to_os_string();
            companion.push(suffix);
            if fs::symlink_metadata(&companion).is_ok_and(|meta| meta.file_type().is_symlink()) {
                return Err("SQLite companion files must not be symbolic links.".into());
            }
        }
        let mut conn = Connection::open(&absolute).map_err(|_| "Cannot open SQLite database.")?;
        conn.busy_timeout(Duration::from_secs(5))
            .map_err(|_| "Cannot configure SQLite timeout.")?;
        inspect_identity(&conn)?;
        conn.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
        )
        .map_err(|_| "Cannot configure SQLite durability.")?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| "Cannot lock database migration.")?;
        // Recheck after acquiring the migration lock in case another process upgraded it.
        let version = inspect_identity(&tx)?;
        if version == 0 {
            tx.execute_batch(include_str!("../migrations/001_accounts.sql"))
                .map_err(|_| "Account migration failed; transaction rolled back.")?;
        }
        if version < 2 {
            tx.execute_batch(include_str!("../migrations/002_campaigns.sql"))
                .map_err(|_| "Campaign migration failed; transaction rolled back.")?;
            tx.pragma_update(None, "user_version", 2)
                .map_err(|_| "Cannot record schema version.")?;
        }
        if version < 3 {
            tx.execute_batch(include_str!("../migrations/003_content.sql"))
                .map_err(|_| "Content migration failed; transaction rolled back.")?;
            tx.pragma_update(None, "application_id", SERVER_ID)
                .map_err(|_| "Cannot record server database identity.")?;
            tx.pragma_update(None, "user_version", 3)
                .map_err(|_| "Cannot record schema version.")?;
        }
        if version < 4 {
            tx.execute_batch(include_str!("../migrations/004_save_receipts.sql"))
                .map_err(|_| "Save receipt migration failed; transaction rolled back.")?;
            tx.pragma_update(None, "user_version", 4)
                .map_err(|_| "Cannot record schema version.")?;
        }
        tx.commit().map_err(|_| "Cannot commit schema migration.")?;
        Ok(Self(Mutex::new(conn)))
    }
    pub fn with<T>(&self, operation: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        let mut conn = self.0.lock().map_err(|_| ApiError::internal())?;
        operation(&mut conn)
    }
    pub fn ready(&self) -> Result<()> {
        self.with(|conn| {
            conn.query_row("SELECT 1", [], |_| Ok(()))?;
            Ok(())
        })
    }
}
