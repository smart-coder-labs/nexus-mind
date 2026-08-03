# Nexus Context Lab: NX-Gold v0

Reproducible, offline harness for context-fabric retrieval and generation experiments.
This package is intentionally isolated from `nexus-local-qa`, production databases,
Docker, network services, and product datasets.

## Safety first

```sh
cd tools/nexus-context-lab
cp .env.example .env
./scripts/preflight --env .env
```

Preflight is fail-closed. It requires an explicitly dedicated workspace, loopback
URLs, a declarative CORS allowlist, fake credentials, a prefetched model and
registered hash, sufficient local resources, and `EGRESS_ENABLED=false`. It never
contacts a service and never starts Docker.

## NX-Gold contract

`schema/nx_gold_v0.schema.json` describes the complete 300-scenario/900-execution
contract. `fixtures/nx_gold_sample.json` is a deliberately incomplete `sample`; it
must never be treated as gold. The deterministic generator creates a complete
structural fixture, but it remains synthetic and can never be promotion evidence.

```sh
./scripts/validate_nx_gold.py fixtures/nx_gold_sample.json
./scripts/validate_nx_gold.py --mode structural fixtures/nx_gold_synthetic_fixture.json
./scripts/validate_nx_gold.py --mode promotion fixtures/nx_gold_synthetic_fixture.json
```

Structural validation is independent from promotion validation. Promotion is
fail-closed for `synthetic_corpus=true`, even when all counts, annotations,
canaries, splits, hashes, and evidence fields are complete.

Generate the offline fixture without network or datasets:

```sh
python3 scripts/generate_nx_gold_fixture.py --output fixtures/nx_gold_synthetic_fixture.json
```

The fixture contains exactly 300 scenarios, three executions per scenario,
three fictitious tenants, 3-5 hard negatives, two annotators, one auditable
adjudication record per scenario, and the full evidence contract. Its canonical
SHA-256 is stored in `fixture_sha256`.

## Offline M1 local run

The local M1 benchmark is isolated and synthetic. It is not the 32GB promotion gate:
every run reports `synthetic_corpus=true`, `nx_gold_status=pending`, and
`promotion=false`. It never downloads a dataset or contacts the network.

Generate a deterministic corpus (the default is 10,000 chunks, three fictitious
tenants, projects, ACLs, metadata, and Float32 embeddings):

```sh
python3 scripts/generate_synthetic_corpus.py --output /tmp/nexus-context-lab-corpus --chunks 10000
```

Run the reproducible quick validation on 10,000 chunks:

```sh
python3 scripts/run_benchmark.py --corpus /tmp/nexus-context-lab-corpus --quick --window 4 --run-root runs
```

The full local protocol uses these defaults with `--protocol`: a 60-second warmup,
five 180-second windows, 20 cold restarts, concurrency 1, AB ordering, and a 95/5
read/update load. Use `--warmup`, `--window`, and `--restarts` as duration/restart
overrides for short validation runs; `--concurrency 1`, `2`, or `4`, `--order AB` or
`BA`, and `--read-update READ/UPDATE` are configurable. For example, this exercises
one stage without waiting hours:

```sh
python3 scripts/run_benchmark.py --protocol --warmup 0 --window 0 --restarts 1 \
  --stages A0 --synthetic-chunks 40 --run-root runs
```

Quick mode remains the small validation protocol and keeps its existing behavior.
`--enable-bq` and `--enable-mrl` are shadow-only and both are off by default. Each
stage runs independently and records actual duration, window/restart IDs, stage
metrics, deterministic seed, artifact completeness, and grouped 10,000-sample
bootstrap metadata with IC95 under `runs/<run-id>/`. Missing stages or interruptions
produce baseline gates; no promotion is activated.

## NX-Gold run

```sh
./scripts/run_nx_gold.py --dataset fixtures/nx_gold_sample.json --run-root runs
```

It writes only non-sensitive manifests and metrics under `runs/<run-id>/` and will
not claim promotion when the dataset or gates are incomplete. The benchmark runner
supports independent A0-A6 stage contracts, deterministic seeds, AB/BA ordering,
warmup/window/restarts/concurrency/read-update parameters, and 10,000-sample grouped
bootstrap metadata with 95% confidence intervals. BQ and MRL default to off.

Stages A0 FTS5-ish lexical, A1 dense Float32, A2 hybrid/RRF, A3 policy-first,
A4 compiler contract, A5 BQ shadow, and A6 MRL shadow execute over the local corpus.
The baseline fallback remains active and theoretical bytes are deliberately separate
from RSS/process measurements. Tool Search MCP remains out of scope.

## Tests

No dependency is required:

```sh
python3 -m unittest discover -s tests -v
```

If the repository already has pytest available, it may also be used, but pytest is
not a package requirement.
