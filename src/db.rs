use anyhow::Result;
use chrono::Utc;
use log::{debug, info};
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Mutex;

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn open(db_path: &str) -> Result<Self> {
        if let Some(parent) = Path::new(db_path).parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS emails (
                id          TEXT PRIMARY KEY,
                subject     TEXT NOT NULL DEFAULT '',
                sender      TEXT NOT NULL DEFAULT '',
                received_at TEXT NOT NULL,
                processed_at TEXT NOT NULL,
                filter      TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE IF NOT EXISTS attachments (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                email_id    TEXT NOT NULL REFERENCES emails(id),
                filename    TEXT NOT NULL,
                path        TEXT NOT NULL,
                size        INTEGER NOT NULL DEFAULT 0,
                mime_type   TEXT NOT NULL DEFAULT '',
                filter      TEXT NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_attachments_email_id ON attachments(email_id);
            CREATE INDEX IF NOT EXISTS idx_emails_received_at ON emails(received_at);",
        )?;
        // Add filter column if missing (safe migration for existing DBs)
        let email_cols: Vec<String> = conn
            .prepare("PRAGMA table_info(emails)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .collect();
        if !email_cols.contains(&"filter".to_string()) {
            conn.execute_batch("ALTER TABLE emails ADD COLUMN filter TEXT NOT NULL DEFAULT '';")?;
        }
        let attachment_cols: Vec<String> = conn
            .prepare("PRAGMA table_info(attachments)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(|r| r.ok())
            .collect();
        if !attachment_cols.contains(&"filter".to_string()) {
            conn.execute_batch("ALTER TABLE attachments ADD COLUMN filter TEXT NOT NULL DEFAULT '';")?;
        }
        if attachment_cols.contains(&"total".to_string()) {
            conn.execute_batch("ALTER TABLE attachments DROP COLUMN total;")?;
            info!("Dropped 'total' column from attachments");
        }
        if attachment_cols.contains(&"parsed_at".to_string()) {
            conn.execute_batch("ALTER TABLE attachments DROP COLUMN parsed_at;")?;
            info!("Dropped 'parsed_at' column from attachments");
        }
        conn.execute_batch("CREATE INDEX IF NOT EXISTS idx_attachments_filter ON attachments(filter);")?;
        info!("Database schema initialized");
        Ok(())
    }

    pub fn email_exists(&self, email_id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM emails WHERE id = ?1",
            params![email_id],
            |row| row.get(0),
        )?;
        debug!("email_exists({}) = {}", email_id, count > 0);
        Ok(count > 0)
    }

    pub fn insert_email(
        &self,
        id: &str,
        subject: &str,
        sender: &str,
        received_at: &str,
        filter: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().to_rfc3339();
        debug!("insert_email: id={}, sender={}, filter={}", id, sender, filter);
        conn.execute(
            "INSERT OR IGNORE INTO emails (id, subject, sender, received_at, processed_at, filter)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, subject, sender, received_at, now, filter],
        )?;
        Ok(())
    }

    pub fn insert_attachment(
        &self,
        email_id: &str,
        filename: &str,
        path: &str,
        size: i64,
        mime_type: &str,
        filter: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        debug!("insert_attachment: email={}, file={}, size={}", email_id, filename, size);
        conn.execute(
            "INSERT INTO attachments (email_id, filename, path, size, mime_type, filter)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![email_id, filename, path, size, mime_type, filter],
        )?;
        Ok(())
    }

    pub fn get_stats(&self) -> Result<(i64, i64)> {
        let conn = self.conn.lock().unwrap();
        let email_count: i64 = conn.query_row("SELECT COUNT(*) FROM emails", [], |row| {
            row.get(0)
        })?;
        let attachment_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM attachments", [], |row| {
                row.get(0)
            })?;
        Ok((email_count, attachment_count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: creates a temporary SQLite DB with the OLD v0.3 schema
    /// (includes `total` and `parsed_at` columns on attachments).
    /// Returns the path — caller is responsible for cleanup.
    fn create_old_schema_db(label: &str) -> String {
        let dir = std::env::temp_dir().join("gmail_fetcher_test");
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join(format!("test_{}.db", label));
        let _ = std::fs::remove_file(&db_path);

        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE emails (
                id          TEXT PRIMARY KEY,
                subject     TEXT NOT NULL DEFAULT '',
                sender      TEXT NOT NULL DEFAULT '',
                received_at TEXT NOT NULL,
                processed_at TEXT NOT NULL,
                filter      TEXT NOT NULL DEFAULT ''
            );
            CREATE TABLE attachments (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                email_id    TEXT NOT NULL REFERENCES emails(id),
                filename    TEXT NOT NULL,
                path        TEXT NOT NULL,
                size        INTEGER NOT NULL DEFAULT 0,
                mime_type   TEXT NOT NULL DEFAULT '',
                total       REAL,
                parsed_at   TEXT,
                filter      TEXT NOT NULL DEFAULT ''
            );",
        )
        .unwrap();

        db_path.to_str().unwrap().to_string()
    }

    /// Helper: returns the column names for a given table via PRAGMA.
    fn get_columns(conn: &Connection, table: &str) -> Vec<String> {
        let pragma = format!("PRAGMA table_info({})", table);
        conn.prepare(&pragma)
            .unwrap()
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    }

    #[test]
    fn migration_drops_total_and_parsed_at() {
        // Step 1: create a DB with the old schema
        let db_path = create_old_schema_db("migration_drops");

        // Step 2: verify the old columns exist before migration
        {
            let conn = Connection::open(&db_path).unwrap();
            let cols = get_columns(&conn, "attachments");
            assert!(cols.contains(&"total".to_string()), "old schema should have 'total'");
            assert!(cols.contains(&"parsed_at".to_string()), "old schema should have 'parsed_at'");
        }

        // Step 3: open through Database::open() — this triggers migrate()
        let db = Database::open(&db_path).unwrap();

        // Step 4: verify the columns are gone
        {
            let conn = db.conn.lock().unwrap();
            let cols = get_columns(&conn, "attachments");
            assert!(!cols.contains(&"total".to_string()), "migration should drop 'total'");
            assert!(!cols.contains(&"parsed_at".to_string()), "migration should drop 'parsed_at'");
            // Sanity check: the columns we care about should still be there
            assert!(cols.contains(&"email_id".to_string()));
            assert!(cols.contains(&"filename".to_string()));
            assert!(cols.contains(&"filter".to_string()));
        }

        // Cleanup
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(format!("{}-wal", db_path));
        let _ = std::fs::remove_file(format!("{}-shm", db_path));
    }

    #[test]
    fn migration_is_idempotent() {
        // Opening the same DB twice should not fail
        let db_path = create_old_schema_db("idempotent");
        let _db1 = Database::open(&db_path).unwrap();
        let db2 = Database::open(&db_path).unwrap();

        // Second open should still have a valid, empty DB
        let conn = db2.conn.lock().unwrap();
        let cols = get_columns(&conn, "attachments");
        assert!(!cols.contains(&"total".to_string()));
        assert!(!cols.contains(&"parsed_at".to_string()));
        assert!(cols.contains(&"email_id".to_string()));

        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(format!("{}-wal", db_path));
        let _ = std::fs::remove_file(format!("{}-shm", db_path));
    }
}
