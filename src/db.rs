use crate::error::{ApiError, Result};
use rusqlite::{Connection, TransactionBehavior};
use std::{
    fs,
    path::{Component, Path},
    sync::Mutex,
    time::Duration,
};
use tokio::sync::broadcast;

/// Content-free hint that a campaign changed. `public` means every member may re-read it.
#[derive(Clone)]
pub struct Signal {
    pub campaign: String,
    pub author: String,
    pub public: bool,
}

pub struct Database(
    pub(crate) Mutex<Connection>,
    pub(crate) broadcast::Sender<Signal>,
);
pub(crate) const SERVER_ID: i64 = 0x43584e53;
/// The newest schema this binary can run; an older binary refuses a newer archive.
pub const LATEST_SCHEMA: u32 = 14;

/// Rejects parent traversal and symbolic links in every existing component of `path`,
/// including the file itself, and returns it as an absolute path.
pub(crate) fn safe_path(path: &Path) -> std::result::Result<std::path::PathBuf, String> {
    let absolute = std::path::absolute(path).map_err(|_| "Invalid database path.")?;
    let mut current = std::path::PathBuf::new();
    for component in absolute.components() {
        if matches!(component, Component::ParentDir) {
            return Err("Database path must not contain parent traversal.".into());
        }
        current.push(component);
        // A Windows drive/extended-path prefix alone is not a path that can be inspected.
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err("Database path must not traverse symbolic links.".into());
            }
            Ok(_) => {}
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("Cannot inspect database path.".into()),
        }
    }
    Ok(absolute)
}
/// A consistent, verified-by-the-caller copy of a database file made without migrating or
/// modifying it. Safe while the server runs.
pub(crate) fn snapshot(source: &Path, destination: &Path) -> std::result::Result<(), String> {
    let conn = Connection::open_with_flags(
        safe_path(source)?,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| "Cannot open the database for a backup.")?;
    conn.busy_timeout(Duration::from_secs(30))
        .map_err(|_| "Cannot configure SQLite timeout.")?;
    conn.execute(
        "VACUUM INTO ?1",
        [destination.to_str().ok_or("Invalid backup path.")?],
    )
    .map_err(|_| "Cannot write the backup file.")?;
    Ok(())
}

fn inspect_identity(conn: &Connection) -> std::result::Result<u32, String> {
    let version: u32 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(|_| "Cannot read schema version.")?;
    let application_id: i64 = conn
        .pragma_query_value(None, "application_id", |r| r.get(0))
        .map_err(|_| "Cannot read database identity.")?;
    if version > LATEST_SCHEMA {
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
        let absolute = safe_path(path)?;
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
        let before = inspect_identity(&conn)?;
        conn.execute_batch(
            "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
        )
        .map_err(|_| "Cannot configure SQLite durability.")?;
        if before > 0 && before < LATEST_SCHEMA {
            // Failed-upgrade recovery: the previous state is copied next to the database
            // first. If that is impossible the upgrade does not start.
            let mut name = absolute.as_os_str().to_os_string();
            name.push(format!(".pre-upgrade-v{before}"));
            let mut backup = std::path::PathBuf::from(name);
            if backup.exists() {
                backup.set_extension(format!("{}", crate::auth::now()));
            }
            conn.execute("VACUUM INTO ?1", [backup.to_str().unwrap_or_default()])
                .map_err(
                    |_| "Cannot write the pre-upgrade backup; the database was not changed.",
                )?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&backup, fs::Permissions::from_mode(0o600));
            }
        }
        if before < 7 {
            conn.pragma_update(None, "foreign_keys", "OFF")
                .map_err(|_| "Cannot prepare content migration.")?;
        }
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
        if version < 5 {
            tx.execute_batch(include_str!("../migrations/005_sync_log.sql"))
                .map_err(|_| "Sync log migration failed; transaction rolled back.")?;
            tx.pragma_update(None, "user_version", 5)
                .map_err(|_| "Cannot record schema version.")?;
        }
        if version < 6 {
            tx.execute_batch(include_str!("../migrations/006_lifecycle.sql"))
                .map_err(|_| "Lifecycle migration failed; transaction rolled back.")?;
            tx.pragma_update(None, "user_version", 6)
                .map_err(|_| "Cannot record schema version.")?;
        }
        if version < 7 {
            tx.execute_batch(include_str!("../migrations/007_clues.sql"))
                .map_err(|_| "Clue migration failed; transaction rolled back.")?;
            tx.pragma_update(None, "user_version", 7)
                .map_err(|_| "Cannot record schema version.")?;
        }
        if version < 8 {
            tx.execute_batch(include_str!("../migrations/008_tables.sql"))
                .map_err(|_| "Table migration failed; transaction rolled back.")?;
            tx.pragma_update(None, "user_version", 8)
                .map_err(|_| "Cannot record schema version.")?;
        }
        if version < 9 {
            tx.execute_batch(include_str!("../migrations/009_gm_access.sql"))
                .map_err(|_| "GM access migration failed; transaction rolled back.")?;
            tx.pragma_update(None, "user_version", 9)
                .map_err(|_| "Cannot record schema version.")?;
        }
        if version < 10 {
            tx.execute_batch(include_str!("../migrations/010_table_live.sql"))
                .map_err(|_| "Table live migration failed; transaction rolled back.")?;
            tx.pragma_update(None, "user_version", 10)
                .map_err(|_| "Cannot record schema version.")?;
        }
        if version < 11 {
            tx.execute_batch(include_str!("../migrations/011_vtt_maps.sql"))
                .map_err(|_| "Table map migration failed; transaction rolled back.")?;
            tx.pragma_update(None, "user_version", 11)
                .map_err(|_| "Cannot record schema version.")?;
        }
        if version < 12 {
            tx.execute_batch(include_str!("../migrations/012_table_combat.sql"))
                .map_err(|_| "Table combat migration failed; transaction rolled back.")?;
            tx.pragma_update(None, "user_version", 12)
                .map_err(|_| "Cannot record schema version.")?;
        }
        if version < 13 {
            tx.execute_batch(include_str!("../migrations/013_table_fog.sql"))
                .map_err(|_| "Table fog migration failed; transaction rolled back.")?;
            tx.pragma_update(None, "user_version", 13)
                .map_err(|_| "Cannot record schema version.")?;
        }
        if version < 14 {
            tx.execute_batch(include_str!("../migrations/014_table_library.sql"))
                .map_err(|_| "Table library migration failed; transaction rolled back.")?;
            tx.pragma_update(None, "user_version", 14)
                .map_err(|_| "Cannot record schema version.")?;
        }
        tx.commit().map_err(|_| "Cannot commit schema migration.")?;
        if before < 7 {
            conn.pragma_update(None, "foreign_keys", "ON")
                .map_err(|_| "Cannot restore foreign key enforcement.")?;
            let violations: i64 = conn
                .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
                    r.get(0)
                })
                .map_err(|_| "Cannot validate migrated foreign keys.")?;
            if violations != 0 {
                return Err("Migrated database has invalid references.".into());
            }
        }
        Ok(Self(Mutex::new(conn), broadcast::channel(64).0))
    }
    pub fn with<T>(&self, operation: impl FnOnce(&mut Connection) -> Result<T>) -> Result<T> {
        let mut conn = self.0.lock().map_err(|_| ApiError::internal())?;
        operation(&mut conn)
    }
    pub fn subscribe(&self) -> broadcast::Receiver<Signal> {
        self.1.subscribe()
    }
    pub(crate) fn notify(&self, campaign: &str, author: &str, public: bool) {
        // No subscriber is not an error; clients fall back to bounded polling.
        let _ = self.1.send(Signal {
            campaign: campaign.into(),
            author: author.into(),
            public,
        });
    }
    pub fn ready(&self) -> Result<()> {
        self.with(|conn| {
            conn.query_row("SELECT 1", [], |_| Ok(()))?;
            Ok(())
        })
    }
}
