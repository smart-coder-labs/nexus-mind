use nexusmind::backup::restore::restore_from_dump;
use nexusmind::backup::serializer::{dump_table, TableDump};
use nexusmind::db::{connection::connect, migrations, queries};
use rusqlite::Connection;
use serde_json::json;

/// Set up an in-memory SQLite with the full schema and a small seed dataset
/// (one org, one user, two memories). Returns the connection.
fn seeded_db() -> Connection {
    let conn = connect(":memory:").expect("connect");
    migrations::run(&conn).expect("migrations");
    let (_org, _user, _key) =
        queries::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
    // Seed two memories so restore has a > 1 row payload.
    conn.execute(
        "INSERT INTO memories (id, org_id, user_id, tool, content) VALUES ('m1', (SELECT id FROM organizations LIMIT 1), (SELECT id FROM users LIMIT 1), 'claude', 'first memory')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO memories (id, org_id, user_id, tool, content) VALUES ('m2', (SELECT id FROM organizations LIMIT 1), (SELECT id FROM users LIMIT 1), 'claude', 'use snake_case for table names')",
        [],
    )
    .unwrap();
    conn
}

#[test]
fn roundtrips_full_database_via_dump_and_restore() {
    let source = seeded_db();
    let mut target = connect(":memory:").expect("connect");
    migrations::run(&target).expect("migrations");

    // Dump every restorable table from the source.
    let dumps: Vec<TableDump> = nexusmind::backup::serializer::BACKUP_TABLES
        .iter()
        .map(|t| dump_table(&source, t).expect("dump"))
        .collect();

    // Convert to the (table_name, rows_value) shape restore_from_dump expects.
    let payload: Vec<(String, serde_json::Value)> = dumps
        .iter()
        .map(|d| (d.table_name.clone(), d.rows.clone()))
        .collect();

    // Apply the restore on a fresh DB.
    let summary = restore_from_dump(&mut target, &payload).expect("restore");
    assert!(summary.total_rows > 0, "restore must copy some rows");

    // The target should now mirror the source. Check a few tables.
    let source_orgs: i64 = source
        .query_row("SELECT COUNT(*) FROM organizations", [], |r| r.get(0))
        .unwrap();
    let target_orgs: i64 = target
        .query_row("SELECT COUNT(*) FROM organizations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(source_orgs, target_orgs);
    assert!(source_orgs >= 1, "source must have at least one org");

    let source_mems: i64 = source
        .query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))
        .unwrap();
    let target_mems: i64 = target
        .query_row("SELECT COUNT(*) FROM memories", [], |r| r.get(0))
        .unwrap();
    assert_eq!(source_mems, target_mems);
    assert_eq!(source_mems, 2, "expect 2 seeded memories");

    // Restore must clear pre-existing rows in the target first. Insert a
    // foreign row into `target` and confirm restore wipes it.
    let mut target = target;
    target
        .execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('o_should_be_wiped', 'Z', 'z')",
            [],
        )
        .unwrap();
    let summary = restore_from_dump(&mut target, &payload).expect("restore again");
    let leftover: i64 = target
        .query_row(
            "SELECT COUNT(*) FROM organizations WHERE id = 'o_should_be_wiped'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(leftover, 0, "restore must DELETE first, then INSERT");
    assert!(summary.total_rows > 0);
}

#[test]
fn restore_rejects_non_array_payload() {
    let mut conn = seeded_db();
    let bad = vec![("memories".to_string(), json!({"not": "an array"}))];
    let err = restore_from_dump(&mut conn, &bad).unwrap_err();
    // anyhow's `Display` impl shows only the outermost context, so use the
    // chain to verify the inner error message is also present.
    let chain: Vec<String> = err.chain().map(|e| e.to_string()).collect();
    let joined = chain.join(" :: ");
    assert!(
        joined.contains("not an array"),
        "expected friendly error, got chain: {joined}"
    );
}

#[test]
fn restore_rejects_non_object_row() {
    let mut conn = seeded_db();
    let bad = vec![("memories".to_string(), json!(["not", "objects"]))];
    let err = restore_from_dump(&mut conn, &bad).unwrap_err();
    let chain: Vec<String> = err.chain().map(|e| e.to_string()).collect();
    let joined = chain.join(" :: ");
    assert!(
        joined.contains("not an object"),
        "expected friendly error, got chain: {joined}"
    );
}

#[test]
fn restore_skips_disallowed_tables() {
    // The `memories_fts` virtual table is not in RESTORABLE_TABLES — passing
    // it in the payload must be a no-op (defensive code, not an error).
    let mut conn = seeded_db();
    let payload = vec![
        ("memories_fts".to_string(), json!([{"rowid": 1, "content": "x"}])),
    ];
    // Should NOT error.
    let _summary = restore_from_dump(&mut conn, &payload).expect("restore allowed-list filter");
}
