# Context Fabric Registry Migration

The Context Fabric registry is migration `v58`. It is additive, idempotent, and
**not applied by backend startup**. Startup continues to run the legacy
migrations and remains compatible while this migration is pending.

## User-applied flow

1. Take the normal SQLite backup and record its checksum.
2. Run `GET /v1/context/migrations` with an authenticated operator key.
3. Apply with `POST /v1/context/migrations`. The caller must have
   `settings:write`; there is no admin bypass in this endpoint.
4. Verify with `GET /v1/context/migrations` and restart the backend.
5. Publish a complete manifest through the versioned Context Fabric profile
   endpoint. Publication validates the manifest and artifact checksums/sizes.

Applying the endpoint a second time is a no-op. A failed apply or publication
does not change the active pointer. Incomplete or private generations are never
returned by the active-generation reader.

## Backup and restore

Back up the SQLite database before applying v58 and retain the artifact files
referenced by each manifest. Restore the database and matching artifact set as a
pair. Verify `GET /v1/context/migrations`, then verify the active manifest and
its checksums before re-enabling profile flags. Do not restore only the pointer
rows or manually edit `cf_active_pointers`.

Rollback is an explicit authenticated request to a previously committed
generation. It only moves the pointer to a validated generation and does not
delete manifests, artifacts, memories, or migration state.

## Flags and safety

The named compatibility profile is `baseline-nomic-768-f32-v1`. New model,
preprocessing, chunker, tokenizer, normalization, or prefix choices must be
explicit in an immutable manifest. No profile is inferred from mutable runtime
defaults, and no generation model calls, caches, BQ/MRL, NX-Gold, or Tool Search
are enabled by this migration.
