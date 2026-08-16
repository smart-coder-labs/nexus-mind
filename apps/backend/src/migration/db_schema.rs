//! The database-schema connector: a client's real data model, turned into prose.
//!
//! A client's model lives in their database, not in their documentation. Table
//! names, foreign keys, CHECK constraints, indexes and comments are the most
//! honest description of their business that exists — and usually the only one
//! that is not out of date.
//!
//! An agent working on a client's project does not know that `invoices.status`
//! accepts exactly five values, or that `orders` references `customers` with
//! `ON DELETE RESTRICT` for a reason. It finds out by breaking something.
//!
//! # Why this file has no SQL in it
//!
//! Everything that can be *wrong* — the four sampling gates, the grouping, the
//! redaction, the prose — lives here, behind [`SchemaReader`]. The I/O half is a
//! separate adapter whose only failure mode is a mistyped column name, which the
//! first real run catches.
//!
//! That is not a testing trick. There is no Postgres in CI, and an integration
//! test marked `#[ignore]` that nobody runs is worse than an honest boundary: it
//! looks like coverage and is not.

use anyhow::Result;
use sha2::{Digest, Sha256};

use super::redact::{redact, RedactionReport};
use super::{CandidatePayload, Connector, ScanOptions, SourceItem};

// ── The shape of a schema, independent of who read it ────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default: Option<String>,
    /// Values a CHECK constraint restricts this column to, if any. A business
    /// rule, not a detail: it is what an agent discovers by breaking something.
    pub accepted_values: Vec<String>,
    pub comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignKey {
    pub column: String,
    pub references_table: String,
    pub references_column: String,
    /// `RESTRICT`, `CASCADE`, `SET NULL`… `ON DELETE RESTRICT` is there for a
    /// reason and the reason is worth carrying.
    pub on_delete: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    pub schema: String,
    pub name: String,
    pub comment: Option<String>,
    pub columns: Vec<Column>,
    pub primary_key: Vec<String>,
    pub foreign_keys: Vec<ForeignKey>,
    pub unique_constraints: Vec<Vec<String>>,
    pub indexes: Vec<String>,
}

impl Table {
    pub fn qualified(&self) -> String {
        format!("{}.{}", self.schema, self.name)
    }
}

/// A table name paired with the redacted rows sampled from it.
pub type TableSample = (String, Vec<Vec<String>>);

/// A row-level security policy. In a Supabase project these are the access
/// rules, and access rules are knowledge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RlsPolicy {
    pub table: String,
    pub name: String,
    pub command: String,
    pub expression: String,
}

/// Reads catalog metadata. Implemented once against Postgres, once in memory.
pub trait SchemaReader {
    fn database_name(&self) -> String;
    /// Host and database only — never the user or the password.
    fn safe_reference(&self) -> String;
    fn tables(&self) -> Result<Vec<Table>>;
    fn rls_policies(&self) -> Result<Vec<RlsPolicy>>;
    /// Tables this connection could write to. Empty means the role is read-only.
    fn writable_tables(&self) -> Result<Vec<String>>;
    /// Deterministically ordered sample. Never called unless all four gates pass.
    fn sample_rows(&self, table: &str, limit: usize) -> Result<Vec<Vec<String>>>;
}

// ── The four gates ───────────────────────────────────────────────────────────

/// What the operator asked for when they asked for data.
#[derive(Debug, Clone, Default)]
pub struct SamplingPolicy {
    pub enabled: bool,
    /// Explicit, table by table. There is deliberately no `--all`.
    pub allowlist: Vec<String>,
    pub limit: Option<usize>,
    pub redact_pii: bool,
    /// Who authorised this and when. Lands on `migration_runs.attestation`.
    pub attestation: Option<String>,
}

/// Why a sampling request was refused. Naming the missing condition is the
/// point: "not authorised" makes the operator guess which of four it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SamplingRefusal {
    NoAllowlist,
    NoLimit,
    NoRedaction,
    NoAttestation,
}

impl SamplingRefusal {
    pub fn as_str(&self) -> &'static str {
        match self {
            SamplingRefusal::NoAllowlist => {
                "--tables is required: sampling needs an explicit table allowlist, never --all"
            }
            SamplingRefusal::NoLimit => "--sample-limit is required: an unbounded sample is a dump",
            SamplingRefusal::NoRedaction => {
                "--redact-pii is required: sampled values must be redacted before they leave this process"
            }
            SamplingRefusal::NoAttestation => {
                "--attest is required: a run that reads client data must record who authorised it"
            }
        }
    }
}

impl SamplingPolicy {
    /// All four conditions, or none. A pure function so the refusal can be
    /// tested without a database — and so the message names what is missing.
    pub fn authorize(&self) -> Result<usize, SamplingRefusal> {
        if self.allowlist.is_empty() {
            return Err(SamplingRefusal::NoAllowlist);
        }
        let Some(limit) = self.limit.filter(|l| *l > 0) else {
            return Err(SamplingRefusal::NoLimit);
        };
        if !self.redact_pii {
            return Err(SamplingRefusal::NoRedaction);
        }
        if self
            .attestation
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
        {
            return Err(SamplingRefusal::NoAttestation);
        }
        Ok(limit)
    }

    pub fn covers(&self, table: &Table) -> bool {
        self.allowlist
            .iter()
            .any(|t| t == &table.name || t == &table.qualified())
    }
}

// ── Grouping ─────────────────────────────────────────────────────────────────

/// Above this, one schema is split by name prefix. A candidate nobody can read
/// is a candidate nobody reviews.
const MAX_TABLES_PER_AREA: usize = 25;

#[derive(Debug, Clone)]
pub struct Area {
    pub schema: String,
    /// `None` when the area is the whole schema; `Some("invoice")` when the
    /// schema was too large and got split by prefix.
    pub prefix: Option<String>,
    pub tables: Vec<Table>,
}

impl Area {
    pub fn label(&self) -> String {
        match &self.prefix {
            Some(p) => format!("{}.{}*", self.schema, p),
            None => self.schema.clone(),
        }
    }
}

/// One candidate per area, not per table.
///
/// Two hundred tables would be two hundred candidates, and the human gate — the
/// bottleneck `repo-docs` measured at 3377 — would be impossible. The area is
/// the Postgres schema, which is how teams actually separate domains; a schema
/// too large for one readable candidate splits by name prefix, the de-facto
/// convention when everything lives in `public`.
pub fn group_into_areas(tables: &[Table]) -> Vec<Area> {
    let mut by_schema: std::collections::BTreeMap<String, Vec<Table>> = Default::default();
    for t in tables {
        by_schema.entry(t.schema.clone()).or_default().push(t.clone());
    }

    let mut areas = Vec::new();
    for (schema, tables) in by_schema {
        if tables.len() <= MAX_TABLES_PER_AREA {
            areas.push(Area {
                schema,
                prefix: None,
                tables,
            });
            continue;
        }
        let mut by_prefix: std::collections::BTreeMap<String, Vec<Table>> = Default::default();
        for t in tables {
            let prefix = t
                .name
                .split_once('_')
                .map(|(head, _)| head.to_string())
                .unwrap_or_else(|| t.name.clone());
            by_prefix.entry(prefix).or_default().push(t);
        }
        for (prefix, tables) in by_prefix {
            areas.push(Area {
                schema: schema.clone(),
                prefix: Some(prefix),
                tables,
            });
        }
    }
    areas
}

// ── Prose ────────────────────────────────────────────────────────────────────

/// Render an area as the description an agent actually needs.
///
/// Raw DDL is not knowledge; it is a table somebody has to read. This answers
/// the questions that cost time: what exists, what values are accepted, what
/// happens on delete.
pub fn render_area(area: &Area, policies: &[RlsPolicy], samples: &[TableSample]) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Schema area: {}\n\n", area.label()));
    out.push_str(&format!(
        "{} table(s): {}\n\n",
        area.tables.len(),
        area.tables
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ));

    for table in &area.tables {
        out.push_str(&format!("## {}\n\n", table.qualified()));
        if let Some(c) = &table.comment {
            out.push_str(&format!("{c}\n\n"));
        }
        if !table.primary_key.is_empty() {
            out.push_str(&format!("Primary key: {}\n", table.primary_key.join(", ")));
        }

        for col in &table.columns {
            let null = if col.nullable { "nullable" } else { "not null" };
            out.push_str(&format!("- `{}` {} ({null})", col.name, col.data_type));
            if let Some(d) = &col.default {
                out.push_str(&format!(", default {d}"));
            }
            if !col.accepted_values.is_empty() {
                // The business rule an agent otherwise learns by breaking things.
                out.push_str(&format!(
                    " — accepts only: {}",
                    col.accepted_values.join(" | ")
                ));
            }
            if let Some(c) = &col.comment {
                out.push_str(&format!(" — {c}"));
            }
            out.push('\n');
        }

        for fk in &table.foreign_keys {
            out.push_str(&format!(
                "- `{}` references `{}`.`{}`, ON DELETE {}\n",
                fk.column, fk.references_table, fk.references_column, fk.on_delete
            ));
        }
        for uq in &table.unique_constraints {
            out.push_str(&format!("- unique: {}\n", uq.join(", ")));
        }

        let table_policies: Vec<&RlsPolicy> =
            policies.iter().filter(|p| p.table == table.name).collect();
        if !table_policies.is_empty() {
            out.push_str("\nAccess policies:\n");
            for p in table_policies {
                out.push_str(&format!("- {} on {}: {}\n", p.name, p.command, p.expression));
            }
        }

        if let Some((_, rows)) = samples.iter().find(|(t, _)| t == &table.name) {
            out.push_str(&format!("\nSampled values ({} row(s), redacted):\n", rows.len()));
            for row in rows {
                out.push_str(&format!("- {}\n", row.join(" | ")));
            }
        }
        out.push('\n');
    }
    out
}

// ── The connector ────────────────────────────────────────────────────────────

pub struct DbSchemaConnector<R: SchemaReader> {
    pub reader: R,
    pub sampling: SamplingPolicy,
    /// Describe row-level security policies as access rules.
    pub supabase: bool,
}

impl<R: SchemaReader> DbSchemaConnector<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            sampling: SamplingPolicy::default(),
            supabase: false,
        }
    }

    pub fn with_sampling(mut self, policy: SamplingPolicy) -> Self {
        self.sampling = policy;
        self
    }

    pub fn with_supabase(mut self, on: bool) -> Self {
        self.supabase = on;
        self
    }

    /// A migration has no reason to be able to write to a client's database. If
    /// it can, that is an incident waiting to happen — so this is verified
    /// rather than trusted to whoever chose the role.
    pub fn ensure_read_only(&self) -> Result<()> {
        let writable = self.reader.writable_tables()?;
        if let Some(table) = writable.first() {
            anyhow::bail!(
                "writable_role: this connection can write to `{table}`. Use a read-only role — \
                 a migration never needs to write to a client's database."
            );
        }
        Ok(())
    }

    fn identity(&self, area: &Area, ddl: &str) -> String {
        let sha = hex::encode(Sha256::digest(ddl.as_bytes()));
        format!(
            "pg:{}:{}:{}",
            self.reader.database_name(),
            area.label(),
            &sha[..16]
        )
    }

    /// Sample only what all four gates allow. Returns `(samples, redaction)`.
    fn samples_for(&self, area: &Area) -> Result<(Vec<TableSample>, RedactionReport)> {
        let mut report = RedactionReport::default();
        if !self.sampling.enabled {
            return Ok((Vec::new(), report));
        }
        let limit = self
            .sampling
            .authorize()
            .map_err(|r| anyhow::anyhow!("sampling_refused: {}", r.as_str()))?;

        let mut out = Vec::new();
        for table in &area.tables {
            if !self.sampling.covers(table) {
                continue;
            }
            let rows = self.reader.sample_rows(&table.qualified(), limit)?;
            let redacted: Vec<Vec<String>> = rows
                .into_iter()
                .map(|row| {
                    row.into_iter()
                        .map(|value| {
                            let (clean, r) = redact(&value);
                            report.home_paths += r.home_paths;
                            report.tokens += r.tokens;
                            report.connection_strings += r.connection_strings;
                            report.emails += r.emails;
                            clean
                        })
                        .collect()
                })
                .collect();
            out.push((table.name.clone(), redacted));
        }
        Ok((out, report))
    }
}

impl<R: SchemaReader> Connector for DbSchemaConnector<R> {
    fn source_kind(&self) -> &'static str {
        "db-schema"
    }

    fn scan(&self, opts: &ScanOptions) -> Result<Vec<SourceItem>> {
        Ok(self.scan_report(opts)?.items)
    }

    fn scan_report(&self, opts: &ScanOptions) -> Result<super::ScanReport> {
        self.ensure_read_only()?;

        let tables = self.reader.tables()?;
        let policies = if self.supabase {
            self.reader.rls_policies()?
        } else {
            Vec::new()
        };

        let mut report = super::ScanReport::default();
        let mut items = Vec::new();

        for (seen, area) in group_into_areas(&tables).into_iter().enumerate() {
            opts.note(seen + 1, area.label());
            let (samples, redaction) = self.samples_for(&area)?;
            let prose = render_area(&area, &policies, &samples);
            let identity = self.identity(&area, &prose);

            items.push(SourceItem {
                source_identity: identity,
                display_origin: format!(
                    "{} — schema area {}",
                    self.reader.database_name(),
                    area.label()
                ),
                raw: prose,
                meta: serde_json::json!({
                    "area": area.label(),
                    "schema": area.schema,
                    "tables": area.tables.iter().map(|t| t.name.clone()).collect::<Vec<_>>(),
                    "sampled": !samples.is_empty(),
                    "redaction": redaction.summary(),
                    "source_ref": self.reader.safe_reference(),
                }),
            });
        }

        report.documents = items.len();
        report.units = items.len();
        report.bytes = items.iter().map(|i| i.raw.len()).sum();
        report.items = items;

        // Say what was NOT read. A default run reads no business row, and the
        // report has to state that rather than leave it to be assumed.
        if !self.sampling.enabled {
            report.excluded.push((
                "business rows".to_string(),
                "no data was sampled — schema-only is the default".to_string(),
            ));
        }
        Ok(report)
    }

    fn classify_prompt(&self, item: &SourceItem) -> String {
        format!(
            "You are describing one area of a client's database schema so it can be PROPOSED — \
             never committed — as team knowledge. A human reviews everything.\n\n\
             ---\n{}\n---\n\n\
             Return ONE JSON object: {{\"source_identity\": \"\", \"destination_kind\": \
             \"memory|skip\", \"content\": \"...\", \"source_excerpt\": \"...\", \
             \"confidence\": 0.0, \"destination_hint\": {{\"title\": \"...\", \"type\": \
             \"architecture\"}}}}\n\n\
             Rules:\n\
             1. PROPOSE, do not decide.\n\
             2. Explain what this area MODELS, in the language of the business — not a restatement \
             of the DDL, which the reader already has above.\n\
             3. Carry the rules forward: accepted values, delete behaviour, uniqueness. Those are \
             the constraints an engineer otherwise learns by breaking something.\n\
             4. `source_excerpt` MUST be copied verbatim from above.\n\
             5. If the area is machine-generated plumbing with no business meaning (migration \
             bookkeeping, job queues), return \"skip\" and say why.",
            item.raw
        )
    }

    fn fallback(&self, item: &SourceItem) -> Option<CandidatePayload> {
        let area = item.meta.get("area").and_then(|v| v.as_str()).unwrap_or("schema");
        Some(CandidatePayload {
            source_identity: item.source_identity.clone(),
            destination_kind: "memory".to_string(),
            content: item.raw.clone(),
            destination_hint: serde_json::json!({
                "title": format!("Data model: {area}"),
                "type": "architecture",
                "tables": item.meta.get("tables"),
                "sampled": item.meta.get("sampled"),
                "redaction": item.meta.get("redaction"),
            }),
            source_excerpt: Some(
                item.raw
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .take(4)
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            confidence: None,
            provenance_kind: Some(if item
                .meta
                .get("sampled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                // Sampled data rests on the operator's attestation, so it is
                // attested rather than verified — and the review UI makes those
                // approve one at a time.
                "client_attested"
            } else {
                "verified_manifest"
            }
            .to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── An in-memory schema, so the logic is testable without Postgres ───────

    #[derive(Default, Clone)]
    struct FakeReader {
        tables: Vec<Table>,
        policies: Vec<RlsPolicy>,
        writable: Vec<String>,
        rows: std::collections::HashMap<String, Vec<Vec<String>>>,
    }

    impl SchemaReader for FakeReader {
        fn database_name(&self) -> String {
            "acme_prod".to_string()
        }
        fn safe_reference(&self) -> String {
            "postgres://db.internal/acme_prod".to_string()
        }
        fn tables(&self) -> Result<Vec<Table>> {
            Ok(self.tables.clone())
        }
        fn rls_policies(&self) -> Result<Vec<RlsPolicy>> {
            Ok(self.policies.clone())
        }
        fn writable_tables(&self) -> Result<Vec<String>> {
            Ok(self.writable.clone())
        }
        fn sample_rows(&self, table: &str, limit: usize) -> Result<Vec<Vec<String>>> {
            Ok(self
                .rows
                .get(table)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .take(limit)
                .collect())
        }
    }

    fn column(name: &str, ty: &str) -> Column {
        Column {
            name: name.to_string(),
            data_type: ty.to_string(),
            nullable: false,
            default: None,
            accepted_values: vec![],
            comment: None,
        }
    }

    fn invoices() -> Table {
        Table {
            schema: "public".to_string(),
            name: "invoices".to_string(),
            comment: Some("One invoice issued to a customer.".to_string()),
            columns: vec![
                column("id", "uuid"),
                Column {
                    accepted_values: vec![
                        "draft".into(),
                        "sent".into(),
                        "paid".into(),
                        "void".into(),
                    ],
                    ..column("status", "text")
                },
                column("customer_id", "uuid"),
            ],
            primary_key: vec!["id".to_string()],
            foreign_keys: vec![ForeignKey {
                column: "customer_id".to_string(),
                references_table: "customers".to_string(),
                references_column: "id".to_string(),
                on_delete: "RESTRICT".to_string(),
            }],
            unique_constraints: vec![vec!["id".to_string()]],
            indexes: vec!["idx_invoices_customer".to_string()],
        }
    }

    fn customers() -> Table {
        Table {
            schema: "public".to_string(),
            name: "customers".to_string(),
            comment: None,
            columns: vec![column("id", "uuid"), column("email", "text")],
            primary_key: vec!["id".to_string()],
            foreign_keys: vec![],
            unique_constraints: vec![],
            indexes: vec![],
        }
    }

    fn reader() -> FakeReader {
        FakeReader {
            tables: vec![invoices(), customers()],
            ..Default::default()
        }
    }

    fn opts() -> ScanOptions {
        ScanOptions::default()
    }

    fn full_policy() -> SamplingPolicy {
        SamplingPolicy {
            enabled: true,
            allowlist: vec!["invoices".to_string()],
            limit: Some(3),
            redact_pii: true,
            attestation: Some("authorised by Cesar on 2026-08-15".to_string()),
        }
    }

    // ── The default reads nothing ────────────────────────────────────────────

    #[test]
    fn default_options_sample_no_rows() {
        let mut r = reader();
        r.rows.insert(
            "public.invoices".to_string(),
            vec![vec!["secret".into(), "paid".into()]],
        );
        let c = DbSchemaConnector::new(r);
        let report = c.scan_report(&opts()).unwrap();

        assert!(report.units > 0);
        for item in &report.items {
            assert_eq!(item.meta["sampled"], serde_json::json!(false));
            assert!(!item.raw.contains("secret"), "no business row may be read by default");
        }
        assert!(
            report
                .excluded
                .iter()
                .any(|(_, r)| r.contains("schema-only is the default")),
            "the report must SAY no data was sampled, not leave it to be assumed"
        );
    }

    // ── Read-only ────────────────────────────────────────────────────────────

    #[test]
    fn a_writable_role_is_refused_naming_the_table() {
        let mut r = reader();
        r.writable = vec!["public.invoices".to_string()];
        let err = DbSchemaConnector::new(r).scan_report(&opts()).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("writable_role"), "{msg}");
        assert!(msg.contains("public.invoices"), "the refusal must name the table: {msg}");
        assert!(msg.contains("read-only"));
    }

    #[test]
    fn a_read_only_role_proceeds() {
        assert!(DbSchemaConnector::new(reader()).scan_report(&opts()).is_ok());
    }

    // ── The four gates, one test each ────────────────────────────────────────

    #[test]
    fn sampling_without_an_allowlist_is_refused() {
        let p = SamplingPolicy {
            allowlist: vec![],
            ..full_policy()
        };
        assert_eq!(p.authorize(), Err(SamplingRefusal::NoAllowlist));
        assert!(p.authorize().unwrap_err().as_str().contains("--tables"));
    }

    #[test]
    fn sampling_without_a_limit_is_refused() {
        let p = SamplingPolicy {
            limit: None,
            ..full_policy()
        };
        assert_eq!(p.authorize(), Err(SamplingRefusal::NoLimit));
        let zero = SamplingPolicy {
            limit: Some(0),
            ..full_policy()
        };
        assert_eq!(zero.authorize(), Err(SamplingRefusal::NoLimit), "0 is not a limit");
    }

    #[test]
    fn sampling_without_redaction_is_refused() {
        let p = SamplingPolicy {
            redact_pii: false,
            ..full_policy()
        };
        assert_eq!(p.authorize(), Err(SamplingRefusal::NoRedaction));
    }

    #[test]
    fn sampling_without_an_attestation_is_refused() {
        for attestation in [None, Some("   ".to_string())] {
            let p = SamplingPolicy {
                attestation,
                ..full_policy()
            };
            assert_eq!(p.authorize(), Err(SamplingRefusal::NoAttestation));
            assert!(p.authorize().unwrap_err().as_str().contains("who authorised"));
        }
    }

    #[test]
    fn all_four_conditions_together_permit_sampling() {
        assert_eq!(full_policy().authorize(), Ok(3));
    }

    #[test]
    fn a_table_outside_the_allowlist_is_never_sampled() {
        let mut r = reader();
        r.rows.insert(
            "public.invoices".to_string(),
            vec![vec!["draft".into()]],
        );
        r.rows.insert(
            "public.customers".to_string(),
            vec![vec!["never-read@example.com".into()]],
        );

        let c = DbSchemaConnector::new(r).with_sampling(full_policy());
        let report = c.scan_report(&opts()).unwrap();
        let prose = &report.items[0].raw;

        assert!(prose.contains("Sampled values"), "the allowlisted table is sampled");
        assert!(
            !prose.contains("never-read"),
            "a table outside the allowlist must not be read at all"
        );
    }

    /// PII is redacted locally, before the sample becomes part of a candidate.
    #[test]
    fn sampled_values_are_redacted_before_they_reach_a_candidate() {
        let mut r = reader();
        r.rows.insert(
            "public.invoices".to_string(),
            vec![vec![
                "ana.lopez@example.com".into(),
                "ghp_abcdefghijklmnopqrstuv".into(),
                "paid".into(),
            ]],
        );
        let c = DbSchemaConnector::new(r).with_sampling(full_policy());
        let report = c.scan_report(&opts()).unwrap();
        let prose = &report.items[0].raw;

        assert!(!prose.contains("ana.lopez@example.com"));
        assert!(!prose.contains("ghp_"));
        assert!(prose.contains("paid"), "non-identifying values survive");
        assert!(report.items[0].meta["redaction"]
            .as_str()
            .unwrap()
            .contains("redacted"));
    }

    /// Sampled data rests on somebody's word, so the review UI makes it be
    /// approved one at a time.
    #[test]
    fn a_sampled_candidate_is_attested_not_verified() {
        let mut r = reader();
        r.rows.insert("public.invoices".to_string(), vec![vec!["paid".into()]]);
        let c = DbSchemaConnector::new(r).with_sampling(full_policy());
        let items = c.scan(&opts()).unwrap();
        let cand = c.fallback(&items[0]).unwrap();
        assert_eq!(cand.provenance_kind.as_deref(), Some("client_attested"));

        let plain = DbSchemaConnector::new(reader());
        let plain_items = plain.scan(&opts()).unwrap();
        assert_eq!(
            plain.fallback(&plain_items[0]).unwrap().provenance_kind.as_deref(),
            Some("verified_manifest")
        );
    }

    // ── Grouping ─────────────────────────────────────────────────────────────

    #[test]
    fn tables_are_grouped_by_schema_not_emitted_one_by_one() {
        let report = DbSchemaConnector::new(reader()).scan_report(&opts()).unwrap();
        assert_eq!(report.units, 1, "two tables in one schema is one candidate");
        let tables = report.items[0].meta["tables"].as_array().unwrap();
        assert_eq!(tables.len(), 2);
    }

    /// Two hundred tables would be two hundred candidates, and the human gate
    /// would be impossible.
    #[test]
    fn a_two_hundred_table_schema_stays_reviewable() {
        let mut r = FakeReader::default();
        for i in 0..200 {
            let prefix = ["invoice", "order", "customer", "audit"][i % 4];
            r.tables.push(Table {
                name: format!("{prefix}_{i}"),
                ..customers()
            });
        }
        let report = DbSchemaConnector::new(r).scan_report(&opts()).unwrap();
        assert!(
            report.units <= 10,
            "200 tables must not become 200 candidates; got {}",
            report.units
        );
        assert!(report.units >= 2, "and they must not collapse into one unreadable blob");
    }

    #[test]
    fn separate_schemas_become_separate_areas() {
        let mut r = reader();
        r.tables.push(Table {
            schema: "auth".to_string(),
            name: "users".to_string(),
            ..customers()
        });
        let report = DbSchemaConnector::new(r).scan_report(&opts()).unwrap();
        assert_eq!(report.units, 2);
        let areas: Vec<&str> = report
            .items
            .iter()
            .map(|i| i.meta["area"].as_str().unwrap())
            .collect();
        assert!(areas.contains(&"auth") && areas.contains(&"public"));
    }

    // ── Constraints as knowledge ─────────────────────────────────────────────

    #[test]
    fn check_constraints_appear_as_accepted_values() {
        let report = DbSchemaConnector::new(reader()).scan_report(&opts()).unwrap();
        let prose = &report.items[0].raw;
        assert!(
            prose.contains("accepts only: draft | sent | paid | void"),
            "the business rule an agent otherwise learns by breaking something:\n{prose}"
        );
    }

    #[test]
    fn restricted_foreign_keys_report_their_delete_behaviour() {
        let report = DbSchemaConnector::new(reader()).scan_report(&opts()).unwrap();
        let prose = &report.items[0].raw;
        assert!(prose.contains("references `customers`.`id`, ON DELETE RESTRICT"), "{prose}");
    }

    #[test]
    fn table_comments_are_carried_forward() {
        let report = DbSchemaConnector::new(reader()).scan_report(&opts()).unwrap();
        assert!(
            report.items[0].raw.contains("One invoice issued to a customer."),
            "the documentation the DBA already wrote and nobody reads"
        );
    }

    // ── Supabase ─────────────────────────────────────────────────────────────

    #[test]
    fn rls_policies_are_described_in_supabase_mode() {
        let mut r = reader();
        r.policies = vec![RlsPolicy {
            table: "invoices".to_string(),
            name: "own_invoices".to_string(),
            command: "SELECT".to_string(),
            expression: "auth.uid() = customer_id".to_string(),
        }];

        let off = DbSchemaConnector::new(r.clone()).scan_report(&opts()).unwrap();
        assert!(!off.items[0].raw.contains("own_invoices"));

        let on = DbSchemaConnector::new(r)
            .with_supabase(true)
            .scan_report(&opts())
            .unwrap();
        assert!(on.items[0].raw.contains("Access policies"));
        assert!(on.items[0].raw.contains("auth.uid() = customer_id"));
    }

    // ── Identity ─────────────────────────────────────────────────────────────

    #[test]
    fn identity_changes_only_for_the_area_whose_ddl_changed() {
        let mut r = reader();
        r.tables.push(Table {
            schema: "auth".to_string(),
            name: "users".to_string(),
            ..customers()
        });
        let before = DbSchemaConnector::new(r.clone()).scan(&opts()).unwrap();

        // A client migration adds a column to public.invoices only.
        r.tables[0].columns.push(column("due_date", "date"));
        let after = DbSchemaConnector::new(r).scan(&opts()).unwrap();

        assert_eq!(before.len(), after.len());
        let changed = before
            .iter()
            .zip(after.iter())
            .filter(|(b, a)| b.source_identity != a.source_identity)
            .count();
        assert_eq!(changed, 1, "only the changed area is re-proposed");
    }

    #[test]
    fn the_run_reference_carries_no_credentials() {
        let report = DbSchemaConnector::new(reader()).scan_report(&opts()).unwrap();
        let reference = report.items[0].meta["source_ref"].as_str().unwrap();
        assert!(reference.contains("acme_prod"), "the database is identified");
        assert!(!reference.contains('@'), "no userinfo may appear: {reference}");
        assert!(!reference.contains("password"));

        for item in &report.items {
            assert!(!item.source_identity.contains('@'));
        }
    }

    #[test]
    fn the_prompt_asks_for_business_meaning_and_allows_skipping() {
        let c = DbSchemaConnector::new(reader());
        let items = c.scan(&opts()).unwrap();
        let prompt = c.classify_prompt(&items[0]);
        assert!(prompt.contains("what this area MODELS"));
        assert!(prompt.contains("not a restatement"));
        assert!(prompt.contains("skip"));
        assert!(prompt.contains("PROPOSE, do not decide"));
    }
}
