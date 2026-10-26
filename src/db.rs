use crate::error::{ApiError, Result};
use rusqlite::{Connection, TransactionBehavior};
use std::{
    fs,
    path::{Component, Path},
    sync::Mutex,
    time::Duration,
};

pub struct Database(pub(crate) Mutex<Connection>);

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
        conn.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
        )
        .map_err(|_| "Cannot configure SQLite durability.")?;
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| "Cannot lock database migration.")?;
        let version: u32 = tx
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(|_| "Cannot read schema version.")?;
        match version {
            0 => {
                let tables: i64 = tx
                    .query_row(
                        "SELECT count(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(|_| "Cannot inspect unversioned database.")?;
                if tables != 0 {
                    return Err("Refusing to migrate an unrecognized existing database.".into());
                }
                tx.execute_batch(include_str!("../migrations/001_accounts.sql"))
                    .map_err(|_| "Account migration failed; original transaction rolled back.")?;
                tx.pragma_update(None, "user_version", 1)
                    .map_err(|_| "Cannot record schema version.")?;
            }
            1 => {}
            _ => {
                return Err(
                    "Database schema is newer than this server; use a compatible binary.".into(),
                );
            }
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
