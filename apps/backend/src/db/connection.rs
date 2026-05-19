use anyhow::Result;
use rusqlite::Connection;

pub fn connect(path: &str) -> Result<Connection> {
    let conn = Connection::open(path)?;
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
    fn foreign_keys_are_enabled() {
        let conn = connect(":memory:").unwrap();
        let fk: i32 = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert_eq!(fk, 1, "foreign_keys should be ON");
    }
}
