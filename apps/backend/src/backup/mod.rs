//! Postgres backup layer for the SQLite store.
//!
//! Architecture:
//!   * `client`     — low-level Postgres reads/writes against the `backups` and
//!     `backup_tables` tables. Connection pooling, schema ensure-on-boot.
//!   * `serializer` — pure logic: read a SQLite table, return a `TableDump`.
//!     Independently testable, no Postgres involved.
//!   * `restore`    — inverse of the serializer: take a list of
//!     `[(table_name, rows)]` and re-insert them into SQLite inside a single
//!     transaction.
//!   * `job`        — orchestration (run one backup, periodic background loop).
//!   * `api::backup` — HTTP handlers (see `apps/backend/src/api/backup.rs`).
//!
//! The connection string is `BACKUP_DATABASE_URL`. If unset, the layer is
//! disabled with a warning — the rest of the app continues normally.

pub mod client;
pub mod job;
pub mod restore;
pub mod serializer;
