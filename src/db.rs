pub use shared_db::Database;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_drops_total_and_parsed_at() {
        let dir = std::env::temp_dir().join("gmail_fetcher_test");
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("test_migration_drops.db");
        let _ = std::fs::remove_file(&db_path);

        // Create old schema
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE emails (id TEXT PRIMARY KEY, subject TEXT, sender TEXT, received_at TEXT, processed_at TEXT, filter TEXT);
                 CREATE TABLE attachments (id INTEGER PRIMARY KEY AUTOINCREMENT, email_id TEXT, filename TEXT, path TEXT, size INTEGER, mime_type TEXT, total REAL, parsed_at TEXT, filter TEXT);",
            ).unwrap();
        }

        let db = Database::open(db_path.to_str().unwrap()).unwrap();
        let conn = db.conn().lock().unwrap();
        let pragma = "PRAGMA table_info(attachments)";
        let cols: Vec<String> = conn.prepare(pragma).unwrap()
            .query_map([], |row| row.get::<_, String>(1)).unwrap()
            .filter_map(|r| r.ok()).collect();
        assert!(!cols.contains(&"total".to_string()));
        assert!(!cols.contains(&"parsed_at".to_string()));
        assert!(cols.contains(&"email_id".to_string()));
        assert!(cols.contains(&"filter".to_string()));

        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(format!("{}-wal", db_path.display()));
        let _ = std::fs::remove_file(format!("{}-shm", db_path.display()));
    }

    #[test]
    fn migration_is_idempotent() {
        let dir = std::env::temp_dir().join("gmail_fetcher_test");
        std::fs::create_dir_all(&dir).unwrap();
        let db_path = dir.join("test_idempotent_gf.db");
        let _ = std::fs::remove_file(&db_path);
        let path_str = db_path.to_str().unwrap().to_string();
        let _db1 = Database::open(&path_str).unwrap();
        let _db2 = Database::open(&path_str).unwrap();
        let _ = std::fs::remove_file(&db_path);
    }
}
