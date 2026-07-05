-- Postgres backup schema for NexusMind SQLite snapshots.
-- Lives in the Supabase Postgres instance (BACKUP_DATABASE_URL) — NOT in the
-- primary SQLite store. Applied on first boot by `backup::ensure_schema`.

CREATE TABLE IF NOT EXISTS backups (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id      TEXT NOT NULL,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    kind        TEXT NOT NULL CHECK (kind IN ('full', 'manual')),
    status      TEXT NOT NULL CHECK (status IN ('pending', 'complete', 'failed')),
    size_bytes  BIGINT,
    error       TEXT,
    metadata    JSONB
);

CREATE INDEX IF NOT EXISTS idx_backups_org_created
    ON backups(org_id, created_at DESC);

CREATE TABLE IF NOT EXISTS backup_tables (
    backup_id    UUID NOT NULL REFERENCES backups(id) ON DELETE CASCADE,
    table_name   TEXT NOT NULL,
    row_count    INTEGER NOT NULL,
    data         JSONB NOT NULL,
    PRIMARY KEY (backup_id, table_name)
);

CREATE INDEX IF NOT EXISTS idx_backup_tables_backup
    ON backup_tables(backup_id);
