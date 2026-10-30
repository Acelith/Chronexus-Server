//! M25.7: consistent backups, integrity verification and restore of the server database.
//!
//! One SQLite file holds accounts, campaigns, content, change logs, receipts and every
//! uploaded original, so one verified file is a complete backup. The server never encrypts
//! or holds backup keys: protect the file with the operator's own tooling (an encrypted
//! volume, `age` or `gpg`) and keep that key outside the backup. A backup contains whatever
//! existed when it was made, including data users deleted since and sessions revoked since;
//! `restore` therefore revokes every session (see docs/INSTALLATION.md).
use crate::db::{Database, LATEST_SCHEMA, SERVER_ID, safe_path};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Report {
    pub schema: u32,
    pub accounts: i64,
    pub campaigns: i64,
    pub content: i64,
    pub attachments: i64,
    pub bytes: u64,
    pub sha256: String,
}
impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "schema {}, {} accounts, {} campaigns, {} content rows, {} uploads, {} bytes, sha256 {}",
            self.schema,
            self.accounts,
            self.campaigns,
            self.content,
            self.attachments,
            self.bytes,
            self.sha256
        )
    }
}

fn file_sha256(path: &Path) -> Result<(u64, String), String> {
    let mut file = fs::File::open(path).map_err(|_| "Cannot read the backup file.")?;
    let (mut hasher, mut size, mut buffer) = (Sha256::new(), 0u64, vec![0u8; 1 << 20]);
    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|_| "Cannot read the backup file.")?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        size += n as u64;
    }
    Ok((size, format!("{:x}", hasher.finalize())))
}
/// Every table, index and trigger with its SQL, as SQLite stores it after migrations.
fn structure(conn: &Connection) -> Result<Vec<(String, String, String)>, String> {
    let mut query = conn
        .prepare(
            "SELECT type,name,coalesce(sql,'') FROM sqlite_master              WHERE name NOT LIKE 'sqlite_%' ORDER BY type,name",
        )
        .map_err(|_| "Cannot read the database structure.")?;
    query
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .and_then(|rows| rows.collect())
        .map_err(|_| "Cannot read the database structure.".into())
}
fn remove_with_companions(path: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let mut file = path.as_os_str().to_os_string();
        file.push(suffix);
        let _ = fs::remove_file(PathBuf::from(file));
    }
}
/// The structure a database of the latest schema must have, built by running every
/// migration on an empty file next to `near`.
fn reference_structure(near: &Path) -> Result<Vec<(String, String, String)>, String> {
    // Unique per call: two verifications in the same process and second must never share
    // (and lock) one reference file.
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = near.join(format!(
        ".reference-{}-{}-{}.tmp",
        std::process::id(),
        crate::auth::now(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let result = Database::open(&path).and_then(|db| {
        let conn = db.0.lock().map_err(|_| "Database lock poisoned.")?;
        structure(&conn)
    });
    remove_with_companions(&path);
    result
}
/// Refuses a database whose tables, indexes or triggers differ from what the latest
/// migrations create, so an integrity-valid but incomplete file is never activated.
fn check_structure(conn: &Connection, near: &Path) -> Result<(), String> {
    if structure(conn)? != reference_structure(near)? {
        return Err(
            "The backup does not have the complete structure of its schema; nothing was changed."
                .into(),
        );
    }
    Ok(())
}
fn count(conn: &Connection, table: &str) -> Result<i64, String> {
    conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .map_err(|_| format!("The backup is missing the {table} table."))
}

/// Opens `path` read-only, without migrating or writing anything, and checks that it is a
/// healthy archive of this server that this binary can run: identity, supported schema,
/// SQLite integrity, foreign keys and the checksum of every stored original.
pub fn verify(path: &Path) -> Result<Report, String> {
    let path = safe_path(path)?;
    let meta = fs::symlink_metadata(&path).map_err(|_| "Backup file not found.")?;
    if !meta.is_file() {
        return Err("The backup must be a regular file.".into());
    }
    let conn = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| "The file is not a readable SQLite database.")?;
    let id: i64 = conn
        .pragma_query_value(None, "application_id", |r| r.get(0))
        .map_err(|_| "Cannot read the database identity.")?;
    let schema: u32 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(|_| "Cannot read the schema version.")?;
    if id != SERVER_ID {
        return Err("This is not a Chronexus Server backup.".into());
    }
    if schema > LATEST_SCHEMA {
        return Err(format!(
            "This backup uses schema {schema}; this server supports up to {LATEST_SCHEMA}. Use a newer server."
        ));
    }
    let integrity: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .map_err(|_| "SQLite integrity check failed to run.")?;
    if integrity != "ok" {
        return Err("The backup is damaged (SQLite integrity check failed).".into());
    }
    let broken: Option<i64> = conn
        .query_row("SELECT 1 FROM pragma_foreign_key_check LIMIT 1", [], |r| {
            r.get(0)
        })
        .optional()
        .map_err(|_| "Foreign key check failed to run.")?;
    if broken.is_some() {
        return Err("The backup has inconsistent references.".into());
    }
    // Older schemas are checked after migration, on the restore staging copy.
    if schema == LATEST_SCHEMA {
        // The reference copy goes to the resolved temporary directory: on macOS it sits
        // behind the /var symbolic link, which database paths refuse.
        let temp = std::env::temp_dir();
        check_structure(&conn, &temp.canonicalize().unwrap_or(temp))?;
    }
    // Every upload is verified against the checksum it was stored with.
    let mut query = conn
        .prepare("SELECT id,sha256,byte_length,original FROM attachments")
        .map_err(|_| "The backup has no uploads table.")?;
    let mut rows = query.query([]).map_err(|_| "Cannot read the uploads.")?;
    while let Some(row) = rows.next().map_err(|_| "Cannot read the uploads.")? {
        let (sha, length, bytes): (String, i64, Vec<u8>) = (
            row.get(1).map_err(|_| "Cannot read an upload.")?,
            row.get(2).map_err(|_| "Cannot read an upload.")?,
            row.get(3).map_err(|_| "Cannot read an upload.")?,
        );
        if bytes.len() as i64 != length || format!("{:x}", Sha256::digest(&bytes)) != sha {
            return Err("An uploaded image in the backup does not match its checksum.".into());
        }
    }
    drop(rows);
    drop(query);
    if schema >= 11 {
        let mut query = conn
            .prepare("SELECT sha256,length(original),original FROM table_map_assets")
            .map_err(|_| "The backup has no table map assets.")?;
        let mut rows = query.query([]).map_err(|_| "Cannot read table maps.")?;
        while let Some(row) = rows.next().map_err(|_| "Cannot read table maps.")? {
            let length: i64 = row.get(1).map_err(|_| "Cannot read a table map.")?;
            if !(1..=crate::vtt::MAX_MAP as i64).contains(&length) {
                return Err("A table map exceeds the supported size limit.".into());
            }
            let sha: String = row.get(0).map_err(|_| "Cannot read a table map.")?;
            let bytes: Vec<u8> = row.get(2).map_err(|_| "Cannot read a table map.")?;
            if format!("{:x}", Sha256::digest(&bytes)) != sha {
                return Err("A table map in the backup does not match its checksum.".into());
            }
        }
    }
    let report = (|| {
        let (bytes, sha256) = file_sha256(&path)?;
        Ok::<_, String>(Report {
            schema,
            accounts: count(&conn, "accounts")?,
            campaigns: count(&conn, "campaigns")?,
            content: count(&conn, "content")?,
            attachments: count(&conn, "attachments")?
                + if schema >= 11 {
                    count(&conn, "table_map_assets")?
                } else {
                    0
                },
            bytes,
            sha256,
        })
    })()?;
    Ok(report)
}

/// Writes a transactionally consistent copy (safe while the server runs) to a new file and
/// verifies it before reporting success. Refuses to overwrite anything.
pub fn create(database: &Path, destination: &Path) -> Result<Report, String> {
    let destination = safe_path(destination)?;
    if destination.exists() {
        return Err("The destination already exists; choose a new file name.".into());
    }
    if destination.parent().is_none_or(|p| !p.is_dir()) {
        return Err("The destination folder does not exist.".into());
    }
    if !database.is_file() {
        return Err("The database file does not exist.".into());
    }
    crate::db::snapshot(database, &destination)?;
    match verify(&destination) {
        Ok(report) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&destination, fs::Permissions::from_mode(0o600));
            }
            Ok(report)
        }
        Err(error) => {
            let _ = fs::remove_file(&destination);
            Err(format!(
                "The new backup failed verification and was removed: {error}"
            ))
        }
    }
}

/// Replaces the database with a verified backup. The server must be stopped. All the
/// work happens on a staging copy: it is migrated, every session in it is ended so revoked
/// access cannot return, and its complete structure and integrity are checked. Only then
/// does it replace the database, which is kept next to it as `.pre-restore`. If any step
/// fails, the active database is left (or put back) as it was.
pub fn restore(database: &Path, backup: &Path) -> Result<(Report, u64, Option<PathBuf>), String> {
    let report = verify(backup)?;
    let database = safe_path(database)?;
    let parent = database
        .parent()
        .ok_or("Invalid database path.")?
        .to_path_buf();
    fs::create_dir_all(&parent).map_err(|_| "Cannot create the data directory.")?;
    let staging = parent.join(format!(".restore-{}.tmp", std::process::id()));
    remove_with_companions(&staging);
    let prepared = (|| {
        fs::copy(backup, &staging).map_err(|_| "Cannot copy the backup into place.")?;
        let (_, copied) = file_sha256(&staging)?;
        if copied != report.sha256 {
            return Err("The copy does not match the backup; nothing was changed.".to_string());
        }
        let ended = {
            // Opening applies newer migrations to the copy only.
            let opened = Database::open(&staging)?;
            let ended = opened.revoke_all_sessions().map_err(|e| e.1.to_string())?;
            let conn = opened.0.lock().map_err(|_| "Database lock poisoned.")?;
            check_structure(&conn, &parent)?;
            // Leave a single self-contained file: no journal travels with the swap.
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA journal_mode=DELETE;")
                .map_err(|_| "Cannot finalize the restored database.")?;
            ended
        };
        verify(&staging)?;
        // Opened for writing: flushing a read-only handle is not allowed on Windows.
        fs::OpenOptions::new()
            .write(true)
            .open(&staging)
            .and_then(|f| f.sync_all())
            .map_err(|_| "Cannot flush the restored file.")?;
        Ok(ended)
    })();
    // The migration of the staging copy may leave its own pre-upgrade copy: drop it, the
    // backup file itself is the original.
    for entry in fs::read_dir(&parent).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(&format!(".restore-{}.tmp.pre-upgrade", std::process::id())) {
            let _ = fs::remove_file(entry.path());
        }
    }
    let ended = match prepared {
        Ok(ended) => ended,
        Err(error) => {
            remove_with_companions(&staging);
            return Err(error);
        }
    };
    let stamp = crate::auth::now();
    let saved = database.with_extension(format!("sqlite3.pre-restore-{stamp}"));
    let mut kept = None;
    if database.exists() {
        fs::rename(&database, &saved).map_err(|_| {
            remove_with_companions(&staging);
            "Cannot keep the current database; nothing was changed."
        })?;
        kept = Some(saved.clone());
    }
    // A journal of the replaced database must never be applied to the new file. It moves
    // along with the kept database so that copy stays self-consistent.
    let mut moved = vec![];
    for suffix in ["-wal", "-shm"] {
        let mut companion = database.as_os_str().to_os_string();
        companion.push(suffix);
        let companion = PathBuf::from(companion);
        if companion.exists() {
            let mut target = saved.clone().into_os_string();
            target.push(suffix);
            let target = PathBuf::from(target);
            if fs::rename(&companion, &target).is_err() {
                undo(&database, kept.as_deref(), &moved);
                remove_with_companions(&staging);
                return Err(
                    "Cannot move the current database journal; nothing was changed.".into(),
                );
            }
            moved.push((companion, target));
        }
    }
    if fs::rename(&staging, &database).is_err() {
        undo(&database, kept.as_deref(), &moved);
        remove_with_companions(&staging);
        return Err(
            "Cannot activate the restored database; the previous one is active again.".into(),
        );
    }
    Ok((report, ended, kept))
}

/// Puts the previous database and its journal back after a failed activation.
fn undo(database: &Path, kept: Option<&Path>, moved: &[(PathBuf, PathBuf)]) {
    for (original, target) in moved {
        let _ = fs::rename(target, original);
    }
    if let Some(kept) = kept {
        let _ = fs::rename(kept, database);
    }
}
