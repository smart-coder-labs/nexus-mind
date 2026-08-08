#!/usr/bin/env python3
"""Offline M1 benchmark over a local synthetic SQLite/JSON corpus."""
from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import math
import os
import random
import sqlite3
import struct
import sys
from pathlib import Path
from statistics import median
from time import perf_counter

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "scripts"))
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from generate_synthetic_corpus import generate
from nxgold import RUN_ARTIFACTS, STAGES, base_manifest, run_dir, validate_clean_room, validate_run_artifacts, write_json

PROTOCOL_DEFAULTS = {"warmup": 60, "window": 180, "restarts": 20, "concurrency": 1, "read_update": "95/5"}


def percentile(values, fraction):
    values = sorted(values)
    if not values:
        return 0.0
    return values[min(len(values) - 1, max(0, math.ceil(len(values) * fraction) - 1))]


def timings(values):
    return {"p50_ms": round(median(values), 4), "p95_ms": round(percentile(values, .95), 4), "samples": len(values)}


def rss_bytes():
    try:
        import resource
        return resource.getrusage(resource.RUSAGE_SELF).ru_maxrss * (1 if sys.platform == "darwin" else 1024)
    except (ImportError, AttributeError):
        return None


def grouped_bootstrap(groups, samples=10_000, seed=20260802):
    rng = random.Random(seed)
    names = sorted(groups)
    values = [groups[name] for name in names]
    estimates = []
    for _ in range(samples):
        estimates.append(sum(values[rng.randrange(len(values))] for _ in values) / len(values))
    estimates.sort()
    return {"method": "grouped", "samples": samples, "seed": seed, "groups": names,
            "ci95": [round(estimates[int(samples * .025)], 6), round(estimates[int(samples * .975) - 1], 6)]}


def run_read_update(records, query_indexes, read_percent, seed):
    rng = random.Random(seed)
    return [{"kind": "read", "id": records[index]["id"]} if rng.randrange(100) < read_percent
            else {"kind": "update", "id": records[index]["id"], "persisted": False} for index in query_indexes]


def load_corpus(path):
    if path.suffix == ".json":
        payload = json.loads(path.read_text(encoding="utf-8"))
        return payload, None
    db_path = path / "corpus.sqlite" if path.is_dir() else path
    connection = sqlite3.connect(str(db_path))
    connection.row_factory = sqlite3.Row
    rows = connection.execute("SELECT id, tenant, project, text, embedding, metadata_json FROM chunks ORDER BY id").fetchall()
    records = [{"id": row["id"], "tenant": row["tenant"], "project": row["project"], "text": row["text"],
                "embedding": list(struct.unpack(f"<{len(row['embedding']) // 4}f", row["embedding"])),
                "metadata": json.loads(row["metadata_json"])} for row in rows]
    return {"schema": "nexus-synthetic-corpus-v1", "synthetic_corpus": True, "chunks": len(records), "records": records}, connection


def lexical(records, query):
    words = set(query.lower().split())
    scored = []
    for record in records:
        score = sum(record["text"].lower().count(word) for word in words)
        if score:
            scored.append((score, record["id"]))
    return [item[1] for item in sorted(scored, key=lambda item: (-item[0], item[1]))]


def dot(left, right):
    return sum(a * b for a, b in zip(left, right))


def query_records(records, index, query_index, k=10, alpha=.5, enable_bq=False, enable_mrl=False):
    query = records[query_index]["embedding"]
    text_query = records[query_index]["text"].split(";")[0]
    oracle = [r["id"] for r in sorted(records, key=lambda r: (-dot(query, r["embedding"]), r["id"]))[:k]]
    candidate_ids = lexical(records, text_query)[: max(k, int(alpha * k + 0.999))]
    dense_ids = [r["id"] for r in sorted(records, key=lambda r: (-dot(query, r["embedding"]), r["id"]))[: max(k, int(alpha * k + 0.999))]]
    lexical_rank = {item: rank for rank, item in enumerate(candidate_ids, 1)}
    dense_rank = {item: rank for rank, item in enumerate(dense_ids, 1)}
    hybrid = sorted(set(candidate_ids) | set(dense_ids), key=lambda item: (-(1 / (60 + lexical_rank.get(item, 10000)) + 1 / (60 + dense_rank.get(item, 10000))), item))[:k]
    allowed = {r["id"] for r in records if r["tenant"] == records[query_index]["tenant"] and r["project"] == records[query_index]["project"]}
    policy = [item for item in hybrid if item in allowed][:k]
    compiler = {"tenant": records[query_index]["tenant"], "project": records[query_index]["project"], "evidence": policy, "contract": "v3"}
    result = {"oracle": oracle, "A0": candidate_ids[:k], "A1": dense_ids[:k], "A2": hybrid, "A3": policy, "A4": compiler}
    if enable_bq:
        bq = sorted(records, key=lambda r: (sum((a >= 0) != (b >= 0) for a, b in zip(query, r["embedding"])), r["id"]))[:k]
        result["A5"] = [r["id"] for r in bq]
    if enable_mrl:
        prefix = max(1, len(query) // 2)
        mrl = sorted(records, key=lambda r: (-dot(query[:prefix], r["embedding"][:prefix]), r["id"]))[:k]
        result["A6"] = [r["id"] for r in mrl]
    return result


def measure_stage(records, query_indexes, stage, concurrency, enable_bq, enable_mrl, duration_seconds=0):
    started_window = perf_counter()
    samples = []
    batch = list(query_indexes) or [0]
    cursor = 0

    def one(index):
        started = perf_counter()
        output = query_records(records, None, index, enable_bq=enable_bq, enable_mrl=enable_mrl)
        latency = (perf_counter() - started) * 1000
        return latency, output
    while True:
        indexes = batch[cursor:] + batch[:cursor]
        cursor = (cursor + len(indexes)) % len(batch)
        with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as pool:
            results = list(pool.map(one, indexes))
        samples.extend(results)
        if duration_seconds <= 0 or perf_counter() - started_window >= duration_seconds:
            break
    latencies = [item[0] for item in samples]
    outputs = [item[1] for item in samples]
    def stage_ids(item):
        value = item[stage]
        return value["evidence"] if isinstance(value, dict) else value
    recalls = [len(set(item["oracle"]) & set(stage_ids(item))) / len(item["oracle"]) for item in outputs]
    return {"latency": timings(latencies), "candidate_recall_at_k": round(sum(recalls) / len(recalls), 6),
            "quality_delta": round(1 - sum(recalls) / len(recalls), 6), "alpha": 0.5,
            "theoretical_bytes": len(samples) * 10 * len(records[0]["embedding"]) * 4,
            "rss_process_bytes": rss_bytes(),
            "duration_seconds": round(perf_counter() - started_window, 6),
            "samples": len(samples)}


def run_warmup(records, query_indexes, seconds, concurrency, enable_bq, enable_mrl):
    started = perf_counter()
    if seconds <= 0:
        return {"requested_seconds": seconds, "duration_seconds": 0.0, "status": "skipped"}
    measure_stage(records, query_indexes, "A0", concurrency, enable_bq, enable_mrl, seconds)
    return {"requested_seconds": seconds, "duration_seconds": round(perf_counter() - started, 6), "status": "complete"}


def main():
    parser = argparse.ArgumentParser(description="Offline deterministic A0-A6 M1 benchmark")
    parser.add_argument("--corpus", type=Path)
    parser.add_argument("--synthetic-chunks", type=int, default=10_000)
    parser.add_argument("--run-root", type=Path, default=Path("runs"))
    parser.add_argument("--seed", type=int, default=20260802)
    parser.add_argument("--warmup", type=int, help="protocol warmup seconds; override for tests")
    parser.add_argument("--window", type=int, help="protocol window seconds; override for tests")
    parser.add_argument("--restarts", type=int, help="protocol cold restarts; override for tests")
    parser.add_argument("--concurrency", type=int, choices=(1, 2, 4), default=1)
    parser.add_argument("--read-update", help="read/update percentages; protocol defaults to 95/5")
    parser.add_argument("--order", choices=("AB", "BA"), default="AB")
    parser.add_argument("--quick", action="store_true", help="small validation protocol; still uses 10k chunks by default")
    parser.add_argument("--protocol", action="store_true", help="full configured protocol")
    parser.add_argument("--enable-bq", action="store_true", help="run BQ shadow only")
    parser.add_argument("--enable-mrl", action="store_true", help="run MRL shadow only")
    parser.add_argument("--stages", default=",".join(STAGES), help="comma-separated independent stages")
    args = parser.parse_args()
    if args.quick:
        args.warmup = 0
        args.window = min(args.window if args.window is not None else 30, 4)
        args.restarts = 1
        args.read_update = args.read_update or "80/20"
    elif args.protocol:
        args.warmup = args.warmup if args.warmup is not None else PROTOCOL_DEFAULTS["warmup"]
        args.window = args.window if args.window is not None else PROTOCOL_DEFAULTS["window"]
        args.restarts = args.restarts if args.restarts is not None else PROTOCOL_DEFAULTS["restarts"]
        args.read_update = args.read_update or PROTOCOL_DEFAULTS["read_update"]
    else:
        args.quick = True
        args.warmup, args.window, args.restarts = 0, min(args.window if args.window is not None else 30, 4), 1
        args.read_update = args.read_update or "80/20"
    try:
        reads, updates = (int(part) for part in args.read_update.split("/"))
        if reads < 0 or updates < 0 or reads + updates != 100:
            raise ValueError
    except (TypeError, ValueError):
        parser.error("--read-update must be a percentage such as 95/5")
    if min(args.warmup, args.window, args.restarts) < 0:
        parser.error("--warmup, --window and --restarts must be non-negative")
    out = run_dir(args.run_root, "benchmark")
    corpus_path = args.corpus or (out / "synthetic-corpus")
    if args.corpus is None:
        generate(corpus_path, args.synthetic_chunks, args.seed)
    payload, connection = load_corpus(corpus_path)
    records = payload["records"]
    if (clean_room_errors := validate_clean_room(payload, require_synthetic=True)):
        raise SystemExit("clean-room rejected: " + "; ".join(clean_room_errors))
    if not records or len(records) != args.synthetic_chunks and args.corpus is None:
        raise SystemExit("corpus chunk count does not match requested synthetic chunks")
    sample_count = max(1, min(args.window, 180))
    query_indexes = [((index * 7919) + args.seed) % len(records) for index in range(sample_count)]
    requested_stages = [stage.strip() for stage in args.stages.split(",") if stage.strip()]
    unknown_stages = sorted(set(requested_stages) - set(STAGES))
    if unknown_stages:
        raise SystemExit("unknown stages: " + ", ".join(unknown_stages))
    if args.enable_bq and "A5" not in requested_stages:
        requested_stages.append("A5")
    if args.enable_mrl and "A6" not in requested_stages:
        requested_stages.append("A6")
    read_update_operations = run_read_update(records, query_indexes, reads, args.seed)
    config = {"seed": args.seed, "synthetic_chunks": len(records), "warmup": args.warmup, "window": args.window,
              "restarts": args.restarts, "concurrency": args.concurrency, "read_update": args.read_update,
              "read_percent": reads, "update_percent": updates,
              "order": args.order, "ab_ba": {"sequence": ["baseline", "candidate"] if args.order == "AB" else ["candidate", "baseline"]},
              "quick": args.quick, "protocol": args.protocol, "bq": args.enable_bq, "mrl": args.enable_mrl,
              "stages": requested_stages, "read_update_operations": read_update_operations}
    stages = {}
    interrupted = None
    try:
        warmup = run_warmup(records, query_indexes, args.warmup, args.concurrency, args.enable_bq, args.enable_mrl)
    except (Exception, KeyboardInterrupt) as error:
        warmup = {"requested_seconds": args.warmup, "duration_seconds": 0.0, "status": "error",
                  "error": f"warmup: {type(error).__name__}: {error}"}
        interrupted = warmup["error"]
    stage_order = list(STAGES)
    if args.order == "BA":
        stage_order.reverse()
    for stage in stage_order:
        if interrupted:
            break
        if stage not in requested_stages:
            stages[stage] = {"enabled": False, "status": "off"}
            continue
        if stage in ("A5", "A6") and not ((stage == "A5" and args.enable_bq) or (stage == "A6" and args.enable_mrl)):
            stages[stage] = {"enabled": False, "status": "off"}
            continue
        stage_data = {"enabled": True, "status": "shadow" if stage in ("A5", "A6") else "measured", "windows": []}
        try:
            for restart_id in range(1, args.restarts + 1):
                for window_id in range(1, 2 if not args.protocol else 6):
                    metrics = measure_stage(records, query_indexes, stage, args.concurrency, args.enable_bq, args.enable_mrl,
                                            args.window if args.protocol else 0)
                    stage_data["windows"].append({
                        "restart_id": f"r{restart_id:02d}", "window_id": f"w{window_id:02d}",
                        "duration_seconds": metrics["duration_seconds"],
                        "requested_duration_seconds": args.window if args.protocol else 0,
                        "metrics": metrics,
                    })
        except (Exception, KeyboardInterrupt) as error:
            interrupted = f"{stage}: {type(error).__name__}: {error}"
            stage_data["status"] = "error"
            stage_data["error"] = interrupted
        completed_metrics = [item["metrics"] for item in stage_data["windows"]]
        if completed_metrics:
            stage_data.update({
                "latency": timings([value for metric in completed_metrics for value in [metric["latency"]["p50_ms"]]]),
                "candidate_recall_at_k": round(sum(metric["candidate_recall_at_k"] for metric in completed_metrics) / len(completed_metrics), 6),
                "quality_delta": round(sum(metric["quality_delta"] for metric in completed_metrics) / len(completed_metrics), 6),
                "alpha": completed_metrics[0]["alpha"],
                "theoretical_bytes": sum(metric["theoretical_bytes"] for metric in completed_metrics),
                "rss_process_bytes": max((metric["rss_process_bytes"] or 0) for metric in completed_metrics),
            })
        stages[stage] = stage_data
        if interrupted:
            break
    if connection:
        connection.close()
    corpus_manifest = corpus_path / "manifest.json" if corpus_path.is_dir() else corpus_path.with_name("manifest.json")
    corpus_hash = hashlib.sha256(json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    manifest = base_manifest(out.name, corpus_hash, "m1-local-synthetic", model="none", resource_id="lab-synthetic")
    manifest["run_metadata"].update({"workspace": str(out.parent.resolve()), "egress_enabled": False,
                                     "order": args.order, "restarts": args.restarts, "concurrency": args.concurrency})
    manifest["generation"]["seed"] = args.seed
    manifest["concurrency"].update({"workers": args.concurrency, "read_update_ratio": args.read_update})
    bootstrap = {}
    for stage, data in stages.items():
        if data.get("enabled") and "candidate_recall_at_k" in data:
            data["bootstrap"] = grouped_bootstrap({tenant: data["candidate_recall_at_k"] for tenant in sorted({r["tenant"] for r in records})}, seed=args.seed)
            bootstrap[stage] = data["bootstrap"]
    manifest.update({"synthetic_corpus": True, "nx_gold_status": "pending", "promotion": False,
                     "dataset_status": "synthetic-not-gold", "stages": stages,
                     "experimental_flags": {"CONTEXT_FABRIC_BQ_ENABLED": "shadow" if args.enable_bq else "off",
                                             "CONTEXT_FABRIC_MRL_ENABLED": "shadow" if args.enable_mrl else "off"},
                      "benchmark": config, "bootstrap": bootstrap, "warmup": warmup,
                      "interrupted": interrupted, "artifact_completeness": {"required": list(RUN_ARTIFACTS), "complete": True},
                      "corpus_manifest": str(corpus_manifest)})
    gates = {"status": "pending", "promotion": False, "fallback": "baseline", "nx_gold_status": "pending",
             "synthetic_corpus": True, "reason": "real NX-Gold v0 remains pending"}
    gates["missing_stages"] = [stage for stage in STAGES if stage not in stages or stages[stage].get("status") in ("off", "missing", "error")]
    gates["interrupted"] = interrupted
    if interrupted:
        gates["reason"] = "benchmark interrupted; baseline remains active"
    write_json(out / "inputs.json", {"schema": "nexus-context-m1-inputs-v1", "corpus": str(corpus_path), "corpus_sha256": corpus_hash, "query_indexes": query_indexes})
    write_json(out / "config.json", {"schema": "nexus-context-m1-config-v1", **config})
    write_json(out / "results.json", {"schema": "nexus-context-m1-v1", "synthetic_corpus": True, "nx_gold_status": "pending",
                                        "promotion": False, "fallback": "baseline", "stages": stages})
    write_json(out / "gates.json", gates)
    write_json(out / "manifest.json", manifest)
    artifact_errors = validate_run_artifacts(out)
    if artifact_errors:
        raise SystemExit("incomplete run artifacts: " + "; ".join(artifact_errors))
    print(out)


if __name__ == "__main__":
    main()
