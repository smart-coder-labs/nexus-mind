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
must never be treated as gold. The validator rejects it as incomplete and reports
`NX-Gold v0: PENDING` until the real 300 scenarios, annotations, canaries, splits,
and evidence are loaded.

```sh
./scripts/validate_nx_gold.py fixtures/nx_gold_sample.json
```

The validator also rejects invalid tenant isolation, leakage, scenario counts,
split totals, missing annotations/adjudication, and missing evidence contracts.

## Offline run

```sh
./scripts/run_nx_gold.py --dataset fixtures/nx_gold_sample.json --run-root runs
```

It writes only non-sensitive manifests and metrics under `runs/<run-id>/` and will
not claim promotion when the dataset or gates are incomplete. The benchmark runner
supports independent A0-A6 stage contracts, deterministic seeds, AB/BA ordering,
warmup/window/restarts/concurrency/read-update parameters, and 10,000-sample grouped
bootstrap metadata with 95% confidence intervals. BQ and MRL default to off.

```sh
./scripts/run_benchmark.py --run-root runs --dataset fixtures/nx_gold_sample.json
```

Stages A0 FTS5, A1 dense Float32, A2 hybrid/RRF, A3 policy-first/generations,
A4 Compiler, A5 BQ shadow, and A6 MRL support deterministic synthetic contract
measurements. They do not access product datasets or claim NX-Gold. BQ/MRL are
shadow-only, dense Float32 is always the final rescore, and theoretical payload
reduction must not be read as RSS or end-to-end improvement. Tool Search MCP remains
out of scope.

## Tests

No dependency is required:

```sh
python3 -m unittest discover -s tests -v
```

If the repository already has pytest available, it may also be used, but pytest is
not a package requirement.
