//! The I/O half of the database connector: `information_schema` → structs.
//!
//! Deliberately thin and deliberately separate from `db_schema.rs`. Everything
//! that can be *wrong in an interesting way* — the sampling gates, the grouping,
//! the redaction, the prose — lives over there and is tested without Postgres.
//!
//! What lives here is a translation whose only failure mode is a mistyped column
//! name, and the first real run against a database catches that. **There is no
//! automated test for this file**, and that is stated rather than papered over
//! with an `#[ignore]`d integration test nobody runs: a test that never executes
//! looks like coverage and is not.

use anyhow::{Context, Result};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};

use super::db_schema::{Column, ForeignKey, RlsPolicy, SchemaReader, Table};

/// Schemas that are Postgres' own plumbing, never a client's business model.
const SYSTEM_SCHEMAS: &[&str] = &[
    "pg_catalog",
    "information_schema",
    "pg_toast",
    "extensions",
    "graphql",
    "graphql_public",
    "pgbouncer",
    "realtime",
    "vault",
];

pub struct PgSchemaReader {
    pool: PgPool,
    runtime: tokio::runtime::Runtime,
    database: String,
    /// `postgres://host/database` — host and database only. Never the user, and
    /// never the password: this is what lands on the run record.
    safe_reference: String,
}

impl PgSchemaReader {
    /// Connect with a read-only intent. The DSN comes from the environment or a
    /// prompt — never from `argv`, where it would survive in shell history, in
    /// `ps`, and in anything that logs commands.
    pub fn connect(dsn: &str) -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("building a runtime for the schema reader")?;

        let pool = runtime.block_on(async {
            PgPoolOptions::new()
                .max_connections(2)
                .connect(dsn)
                .await
                .context("connecting to the source database")
        })?;

        let database: String = runtime
            .block_on(sqlx::query("SELECT current_database()").fetch_one(&pool))
            .context("reading the database name")?
            .get(0);

        Ok(Self {
            safe_reference: safe_reference_for(dsn, &database),
            pool,
            runtime,
            database,
        })
    }
}

/// Strip everything identifying from a DSN, keeping host and database.
///
/// `postgres://user:pw@db.internal:5432/prod` → `postgres://db.internal/prod`.
pub fn safe_reference_for(dsn: &str, database: &str) -> String {
    let after_scheme = dsn.split("://").nth(1).unwrap_or(dsn);
    let authority = after_scheme.split('/').next().unwrap_or("");
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = host.split(':').next().unwrap_or(host);
    format!("postgres://{host}/{database}")
}

impl SchemaReader for PgSchemaReader {
    fn database_name(&self) -> String {
        self.database.clone()
    }

    fn safe_reference(&self) -> String {
        self.safe_reference.clone()
    }

    fn tables(&self) -> Result<Vec<Table>> {
        let excluded = SYSTEM_SCHEMAS.join("','");
        let sql = format!(
            "SELECT c.table_schema, c.table_name, c.column_name, c.data_type,
                    c.is_nullable, c.column_default,
                    pgd.description AS column_comment,
                    obj_description(fc.oid) AS table_comment
               FROM information_schema.columns c
               JOIN pg_class fc
                 ON fc.relname = c.table_name
               JOIN pg_namespace ns
                 ON ns.oid = fc.relnamespace AND ns.nspname = c.table_schema
               LEFT JOIN pg_description pgd
                 ON pgd.objoid = fc.oid AND pgd.objsubid = c.ordinal_position
              WHERE c.table_schema NOT IN ('{excluded}')
              ORDER BY c.table_schema, c.table_name, c.ordinal_position"
        );

        let rows = self
            .runtime
            .block_on(sqlx::query(&sql).fetch_all(&self.pool))
            .context("reading columns")?;

        let mut tables: Vec<Table> = Vec::new();
        for row in rows {
            let schema: String = row.get("table_schema");
            let name: String = row.get("table_name");
            let column = Column {
                name: row.get("column_name"),
                data_type: row.get("data_type"),
                nullable: row.get::<String, _>("is_nullable") == "YES",
                default: row.try_get("column_default").ok(),
                accepted_values: vec![],
                comment: row.try_get("column_comment").ok(),
            };
            match tables
                .iter_mut()
                .find(|t| t.schema == schema && t.name == name)
            {
                Some(t) => t.columns.push(column),
                None => tables.push(Table {
                    schema,
                    name,
                    comment: row.try_get("table_comment").ok(),
                    columns: vec![column],
                    primary_key: vec![],
                    foreign_keys: vec![],
                    unique_constraints: vec![],
                    indexes: vec![],
                }),
            }
        }

        self.attach_keys(&mut tables)?;
        self.attach_check_values(&mut tables)?;
        Ok(tables)
    }

    fn rls_policies(&self) -> Result<Vec<RlsPolicy>> {
        let rows = self
            .runtime
            .block_on(
                sqlx::query(
                    "SELECT tablename, policyname, cmd, COALESCE(qual, '') AS qual
                       FROM pg_policies
                      ORDER BY tablename, policyname",
                )
                .fetch_all(&self.pool),
            )
            .context("reading row-level security policies")?;

        Ok(rows
            .into_iter()
            .map(|r| RlsPolicy {
                table: r.get("tablename"),
                name: r.get("policyname"),
                command: r.get("cmd"),
                expression: r.get("qual"),
            })
            .collect())
    }

    /// Tables this connection could write to.
    ///
    /// Verified, not trusted: whoever ran this may have reached for the wrong
    /// role, and that is exactly the mistake the check exists to catch.
    fn writable_tables(&self) -> Result<Vec<String>> {
        let excluded = SYSTEM_SCHEMAS.join("','");
        let sql = format!(
            "SELECT table_schema || '.' || table_name AS qualified
               FROM information_schema.tables
              WHERE table_schema NOT IN ('{excluded}')
                AND table_type = 'BASE TABLE'
                AND has_table_privilege(
                      quote_ident(table_schema) || '.' || quote_ident(table_name),
                      'INSERT')
              ORDER BY 1
              LIMIT 5"
        );
        let rows = self
            .runtime
            .block_on(sqlx::query(&sql).fetch_all(&self.pool))
            .context("checking write privileges")?;
        Ok(rows.into_iter().map(|r| r.get("qualified")).collect())
    }

    /// Deterministic sample: ordered by primary key, never `RANDOM()`.
    ///
    /// Two runs over unchanged data must produce the same sample, or the
    /// pipeline's idempotency — which keys on a content hash — breaks.
    fn sample_rows(&self, table: &str, limit: usize) -> Result<Vec<Vec<String>>> {
        let sql = format!("SELECT * FROM {table} ORDER BY 1 LIMIT {limit}");
        let rows = self
            .runtime
            .block_on(sqlx::query(&sql).fetch_all(&self.pool))
            .with_context(|| format!("sampling {table}"))?;

        Ok(rows
            .into_iter()
            .map(|row| {
                (0..row.len())
                    .map(|i| {
                        row.try_get::<String, _>(i)
                            .or_else(|_| row.try_get::<i64, _>(i).map(|v| v.to_string()))
                            .unwrap_or_else(|_| "<unrenderable>".to_string())
                    })
                    .collect()
            })
            .collect())
    }
}

impl PgSchemaReader {
    fn attach_keys(&self, tables: &mut [Table]) -> Result<()> {
        let rows = self
            .runtime
            .block_on(
                sqlx::query(
                    "SELECT tc.table_schema, tc.table_name, tc.constraint_type,
                            kcu.column_name,
                            ccu.table_name  AS foreign_table,
                            ccu.column_name AS foreign_column,
                            rc.delete_rule
                       FROM information_schema.table_constraints tc
                       JOIN information_schema.key_column_usage kcu
                         ON kcu.constraint_name = tc.constraint_name
                        AND kcu.table_schema = tc.table_schema
                       LEFT JOIN information_schema.constraint_column_usage ccu
                         ON ccu.constraint_name = tc.constraint_name
                       LEFT JOIN information_schema.referential_constraints rc
                         ON rc.constraint_name = tc.constraint_name
                      WHERE tc.constraint_type IN ('PRIMARY KEY','FOREIGN KEY','UNIQUE')
                      ORDER BY tc.table_schema, tc.table_name",
                )
                .fetch_all(&self.pool),
            )
            .context("reading keys and constraints")?;

        for row in rows {
            let schema: String = row.get("table_schema");
            let name: String = row.get("table_name");
            let Some(table) = tables
                .iter_mut()
                .find(|t| t.schema == schema && t.name == name)
            else {
                continue;
            };
            let column: String = row.get("column_name");
            match row.get::<String, _>("constraint_type").as_str() {
                "PRIMARY KEY" => table.primary_key.push(column),
                "UNIQUE" => table.unique_constraints.push(vec![column]),
                "FOREIGN KEY" => table.foreign_keys.push(ForeignKey {
                    column,
                    references_table: row.try_get("foreign_table").unwrap_or_default(),
                    references_column: row.try_get("foreign_column").unwrap_or_default(),
                    on_delete: row
                        .try_get::<String, _>("delete_rule")
                        .unwrap_or_else(|_| "NO ACTION".to_string()),
                }),
                _ => {}
            }
        }
        Ok(())
    }

    /// Pull the accepted values out of `CHECK (col IN ('a','b'))`.
    ///
    /// A hand parser rather than a full expression parser: the enumerated-column
    /// shape is what carries business meaning, and anything more exotic is
    /// better left to the reviewer than half-understood.
    fn attach_check_values(&self, tables: &mut [Table]) -> Result<()> {
        let rows = self
            .runtime
            .block_on(
                sqlx::query(
                    "SELECT n.nspname AS schema, rel.relname AS table_name,
                            pg_get_constraintdef(con.oid) AS definition
                       FROM pg_constraint con
                       JOIN pg_class rel ON rel.oid = con.conrelid
                       JOIN pg_namespace n ON n.oid = rel.relnamespace
                      WHERE con.contype = 'c'",
                )
                .fetch_all(&self.pool),
            )
            .context("reading check constraints")?;

        for row in rows {
            let schema: String = row.get("schema");
            let name: String = row.get("table_name");
            let definition: String = row.get("definition");
            let Some(table) = tables
                .iter_mut()
                .find(|t| t.schema == schema && t.name == name)
            else {
                continue;
            };
            if let Some((column, values)) = parse_check_in(&definition) {
                if let Some(col) = table.columns.iter_mut().find(|c| c.name == column) {
                    col.accepted_values = values;
                }
            }
        }
        Ok(())
    }
}

/// `CHECK ((status = ANY (ARRAY['draft'::text, 'sent'::text])))` → the column
/// and its accepted values.
pub fn parse_check_in(definition: &str) -> Option<(String, Vec<String>)> {
    let inner = definition.split_once("CHECK")?.1;
    let column: String = inner
        .chars()
        .skip_while(|c| !c.is_alphanumeric() && *c != '_')
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if column.is_empty() {
        return None;
    }
    let mut values = Vec::new();
    let mut rest = inner;
    while let Some(open) = rest.find('\'') {
        let after = &rest[open + 1..];
        let close = after.find('\'')?;
        let value = &after[..close];
        if !value.is_empty() && !values.contains(&value.to_string()) {
            values.push(value.to_string());
        }
        rest = &after[close + 1..];
    }
    if values.is_empty() {
        return None;
    }
    Some((column, values))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two pure functions in this file are the only ones worth testing here;
    /// everything else is I/O whose failure mode the first real run catches.
    #[test]
    fn a_dsn_is_reduced_to_host_and_database() {
        for (dsn, expected) in [
            ("postgres://admin:hunter2@db.internal:5432/prod", "postgres://db.internal/acme"),
            ("postgresql://db.internal/prod", "postgres://db.internal/acme"),
            ("postgres://user@localhost:5432/x", "postgres://localhost/acme"),
        ] {
            let out = safe_reference_for(dsn, "acme");
            assert_eq!(out, expected, "for {dsn}");
            assert!(!out.contains("hunter2"));
            assert!(!out.contains('@'), "no userinfo may survive: {out}");
        }
    }

    #[test]
    fn check_constraints_yield_their_accepted_values() {
        let def = "CHECK (((status)::text = ANY ((ARRAY['draft'::character varying, \
                   'sent'::character varying, 'paid'::character varying])::text[])))";
        let (column, values) = parse_check_in(def).unwrap();
        assert_eq!(column, "status");
        assert_eq!(values, vec!["draft", "sent", "paid"]);
    }

    #[test]
    fn a_check_with_no_literals_yields_nothing() {
        assert!(parse_check_in("CHECK ((amount > 0))").is_none());
    }
}
