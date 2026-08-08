# Context Fabric Registry Migration

The Context Fabric registry is migration `v58`. It is additive, idempotent, and
**not applied by backend startup**. Startup continues to run the legacy
migrations and remains compatible while this migration is pending.

Context Fabric provenance is migration `v59`. It adds the nullable
`memories.context_fabric_metadata` column, remains compatible with
`memory-schema-v2`, and is never run at startup.

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

For provenance use `GET /v1/context/migrations/provenance`, apply with
`POST /v1/context/migrations/provenance`, and verify with
`POST /v1/context/migrations/provenance/verify`. The v59 endpoint requires the
v58 registry first. Invalid metadata is atomic; the backend owns `trusted` and
`verified`, so client claims cannot set either flag.

## Backup and restore

Back up the SQLite database before applying v58 and retain the artifact files
referenced by each manifest. Restore the database and matching artifact set as a
pair. Verify `GET /v1/context/migrations`, then verify the active manifest and
its checksums before re-enabling profile flags. Do not restore only the pointer
rows or manually edit `cf_active_pointers`.

Back up again before applying v59. The provenance column is included in the
normal `memories` backup and restore allow-list. Restore the database snapshot
as a whole, then verify v59 before accepting provenance writes. Never copy
metadata into a separate sidecar or across tenants.

Rollback is an explicit authenticated request to a previously committed
generation. It only moves the pointer to a validated generation and does not
delete manifests, artifacts, memories, or migration state.

## Flags and safety

The named compatibility profile is `baseline-nomic-768-f32-v1`. New model,
preprocessing, chunker, tokenizer, normalization, or prefix choices must be
explicit in an immutable manifest. No profile is inferred from mutable runtime
defaults, and no generation model calls, caches, BQ/MRL, NX-Gold, or Tool Search
are enabled by this migration.

## v60 BQ/MRL sidecar

Migration `v60` is additive and user-applied after v59. Check
`GET /v1/context/migrations/sidecar`, apply with
`POST /v1/context/migrations/sidecar`, and verify with
`POST /v1/context/migrations/sidecar/verify`; write operations require
`settings:write`.

The sidecar stores only sign bits and identity metadata associated with `memory_id`, profile,
generation, dimension/bits, source hash, ACL/policy generations, build manifest/checksum, and
status. It never replaces `memory_embeddings` Float32 or duplicates memory content. Rebuild is
explicit with `POST /v1/context/sidecar/rebuild`; it uses authorized 768-dimension Float32 rows
without model loading or network access. Repeating a generation is idempotent and
`POST /v1/context/sidecar/rebuild/cancel` cancels that generation.

Status, verification, and tombstone cleanup are exposed through
`/v1/context/sidecar/status`, `/verify`, and `/cleanup`. Memory update/delete/archive/restore
tombstones sidecars, and shadow queries never serve a stale, unauthorized, incompatible, or
cross-generation row.

`POST /v1/context/sidecar/shadow` is enabled only when the capability flag is exactly `shadow`.
It filters tenant/ACL/policy and generation identity before Hamming candidate generation, then
rescoring uses authorized dense Float32 vectors. It always reports baseline fallback and never
promotes automatically. RSS, end-to-end, and NX-Gold gates remain pending; no large benchmark is
claimed here.
