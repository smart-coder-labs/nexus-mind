#!/usr/bin/env python3
"""Offline M1 benchmark over a local synthetic SQLite/JSON corpus."""
from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import math
import os
import sqlite3
import struct
import sys
from pathlib import Path
from statistics import median
from time import perf_counter

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "scripts"))
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from generate_synthetic_corpus import generate
from nxgold import STAGES, base_manifest, canonical_hash, run_dir, write_json


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
    return {"schema": "nexus-synthetic-corpus-v1", "chunks": len(records), "records": records}, connection


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


def measure_stage(records, query_indexes, stage, concurrency, enable_bq, enable_mrl):
    def one(index):
        started = perf_counter()
        output = query_records(records, None, index, enable_bq=enable_bq, enable_mrl=enable_mrl)
        latency = (perf_counter() - started) * 1000
        return latency, output
    with concurrent.futures.ThreadPoolExecutor(max_workers=concurrency) as pool:
        results = list(pool.map(one, query_indexes))
    latencies = [item[0] for item in results]
    outputs = [item[1] for item in results]
    def stage_ids(item):
        value = item[stage]
        return value["evidence"] if isinstance(value, dict) else value
    recalls = [len(set(item["oracle"]) & set(stage_ids(item))) / len(item["oracle"]) for item in outputs]
    return {"latency": timings(latencies), "candidate_recall_at_k": round(sum(recalls) / len(recalls), 6),
            "quality_delta": round(1 - sum(recalls) / len(recalls), 6), "alpha": 0.5,
            "theoretical_bytes": len(query_indexes) * 10 * len(records[0]["embedding"]) * 4,
            "rss_process_bytes": rss_bytes()}


def main():
    parser = argparse.ArgumentParser(description="Offline deterministic A0-A6 M1 benchmark")
    parser.add_argument("--corpus", type=Path)
    parser.add_argument("--synthetic-chunks", type=int, default=10_000)
    parser.add_argument("--run-root", type=Path, default=Path("runs"))
    parser.add_argument("--seed", type=int, default=20260802)
    parser.add_argument("--warmup", type=int, default=2)
    parser.add_argument("--window", type=int, default=30)
    parser.add_argument("--restarts", type=int, default=3)
    parser.add_argument("--concurrency", type=int, choices=(1, 2, 4), default=1)
    parser.add_argument("--read-update", default="80/20")
    parser.add_argument("--order", choices=("AB", "BA"), default="AB")
    parser.add_argument("--quick", action="store_true", help="small validation protocol; still uses 10k chunks by default")
    parser.add_argument("--protocol", action="store_true", help="full configured protocol")
    parser.add_argument("--enable-bq", action="store_true", help="run BQ shadow only")
    parser.add_argument("--enable-mrl", action="store_true", help="run MRL shadow only")
    args = parser.parse_args()
    try:
        reads, updates = (int(part) for part in args.read_update.split("/"))
        if reads < 0 or updates < 0 or reads + updates != 100:
            raise ValueError
    except ValueError:
        parser.error("--read-update must be a percentage such as 80/20")
    if args.quick:
        args.warmup, args.window, args.restarts = 0, min(args.window, 4), 1
    if not args.quick and not args.protocol:
        args.quick = True
        args.warmup, args.window, args.restarts = 0, min(args.window, 4), 1
    out = run_dir(args.run_root, "benchmark")
    corpus_path = args.corpus or (out / "synthetic-corpus")
    if args.corpus is None:
        generate(corpus_path, args.synthetic_chunks, args.seed)
    payload, connection = load_corpus(corpus_path)
    records = payload["records"]
    if not records or len(records) != args.synthetic_chunks and args.corpus is None:
        raise SystemExit("corpus chunk count does not match requested synthetic chunks")
    base_query_indexes = [((index * 7919) + args.seed) % len(records) for index in range(args.window)]
    query_indexes = base_query_indexes * max(1, args.restarts)
    config = {"seed": args.seed, "synthetic_chunks": len(records), "warmup": args.warmup, "window": args.window,
              "restarts": args.restarts, "concurrency": args.concurrency, "read_update": args.read_update,
              "read_percent": reads, "update_percent": updates,
              "order": args.order, "quick": args.quick, "protocol": args.protocol, "bq": args.enable_bq, "mrl": args.enable_mrl}
    stages = {}
    stage_order = list(STAGES)
    if args.order == "BA":
        stage_order.reverse()
    for _ in range(args.warmup):
        query_records(records, None, query_indexes[0], enable_bq=args.enable_bq, enable_mrl=args.enable_mrl)
    for stage in stage_order:
        if stage in ("A5", "A6") and not ((stage == "A5" and args.enable_bq) or (stage == "A6" and args.enable_mrl)):
            stages[stage] = {"enabled": False, "status": "off"}
            continue
        stages[stage] = {"enabled": True, "status": "shadow" if stage in ("A5", "A6") else "measured",
                         **measure_stage(records, query_indexes, stage, args.concurrency, args.enable_bq, args.enable_mrl)}
    if connection:
        connection.close()
    corpus_manifest = corpus_path / "manifest.json" if corpus_path.is_dir() else corpus_path.with_name("manifest.json")
    corpus_hash = hashlib.sha256(json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    manifest = base_manifest(out.name, corpus_hash, "m1-local-synthetic", model="none")
    manifest.update({"synthetic_corpus": True, "nx_gold_status": "pending", "promotion": False,
                     "dataset_status": "synthetic-not-gold", "stages": stages,
                     "experimental_flags": {"CONTEXT_FABRIC_BQ_ENABLED": "shadow" if args.enable_bq else "off",
                                             "CONTEXT_FABRIC_MRL_ENABLED": "shadow" if args.enable_mrl else "off"},
                     "benchmark": config, "corpus_manifest": str(corpus_manifest)})
    gates = {"status": "pending", "promotion": False, "fallback": "baseline", "nx_gold_status": "pending",
             "synthetic_corpus": True, "reason": "real NX-Gold v0 remains pending"}
    write_json(out / "inputs.json", {"corpus": str(corpus_path), "corpus_sha256": corpus_hash, "query_indexes": query_indexes})
    write_json(out / "config.json", config)
    write_json(out / "results.json", {"schema": "nexus-context-m1-v1", "synthetic_corpus": True, "nx_gold_status": "pending",
                                       "promotion": False, "fallback": "baseline", "stages": stages})
    write_json(out / "gates.json", gates)
    write_json(out / "manifest.json", manifest)
    print(out)


if __name__ == "__main__":
    main()
