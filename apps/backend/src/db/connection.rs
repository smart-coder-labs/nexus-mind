use anyhow::Result;
use rusqlite::Connection;

/// How long a statement waits for another process's write lock before failing
/// with SQLITE_BUSY. The backend and the autonomous worker share the database
/// file; without a wait, any overlap is an immediate error.
pub const BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// How long startup migrations wait: a migration that rebuilds an index can hold
/// the lock for seconds while the other container starts.
pub const MIGRATION_BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

pub fn connect(path: &str) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.busy_timeout(BUSY_TIMEOUT)?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA foreign_keys=ON;
         PRAGMA cache_size=-8000;",
    )?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connect_in_memory_succeeds() {
        let conn = connect(":memory:").unwrap();
        let result: i32 = conn
            .query_row("SELECT 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(result, 1);
    }

    #[test]
    fn a_writer_waits_for_another_connections_lock() {
        let path = std::env::temp_dir().join(format!("busy-{}.db", std::process::id()));
        let path = path.to_str().unwrap();
        let first = connect(path).unwrap();
        first.execute_batch("CREATE TABLE IF NOT EXISTS t (x INTEGER); BEGIN IMMEDIATE;").unwrap();
        let waiter = std::thread::spawn({
            let path = path.to_string();
            move || connect(&path).unwrap().execute("INSERT INTO t VALUES (1)", [])
        });
        std::thread::sleep(std::time::Duration::from_millis(300));
        first.execute_batch("COMMIT;").unwrap();
        assert!(waiter.join().unwrap().is_ok(), "the second writer waited instead of SQLITE_BUSY");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn foreign_keys_are_enabled() {
        let conn = connect(":memory:").unwrap();
        let fk: i32 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1, "foreign_keys should be ON");
    }
}
