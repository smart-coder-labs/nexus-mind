#!/usr/bin/env python3
import argparse, sys
from time import perf_counter
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from nxgold import STAGES, base_manifest, canonical_hash, load_json, run_dir, validate_dataset, write_json

parser = argparse.ArgumentParser(description="Offline deterministic A0-A6 benchmark contract runner")
parser.add_argument("--dataset", type=Path, required=True)
parser.add_argument("--run-root", type=Path, default=Path("runs"))
parser.add_argument("--seed", type=int, default=20260802)
parser.add_argument("--warmup", type=int, default=2)
parser.add_argument("--window", type=int, default=30)
parser.add_argument("--restarts", type=int, default=3)
parser.add_argument("--concurrency", type=int, default=1)
parser.add_argument("--read-update", default="80/20")
parser.add_argument("--order", choices=("AB", "BA"), default="AB")
parser.add_argument("--enable-bq", action="store_true")
parser.add_argument("--enable-mrl", action="store_true")
args = parser.parse_args()


def synthetic_shadow(capability, alpha=2.0, k=4, dimension=768):
    """Run a deterministic synthetic contract measurement, never a gold claim."""
    query = [1.0] * dimension
    vectors = []
    for index in range(32):
        vectors.append((f"synthetic-{index:03d}", [1.0 if (index == 0 or value % 17 == 0) else -1.0 for value in range(dimension)]))
    def dense_distance(vector, prefix=dimension):
        dot = sum(query[i] * vector[i] for i in range(prefix))
        return -dot
    baseline = [item[0] for item in sorted(vectors, key=lambda item: (dense_distance(item[1]), item[0]))][:k]
    candidate_start = perf_counter()
    if capability == "bq":
        def bq_distance(vector):
            return sum((query[i] >= 0) != (vector[i] >= 0) for i in range(dimension))
        candidates = [item[0] for item in sorted(vectors, key=lambda item: (bq_distance(item[1]), item[0]))]
        payload = ((dimension + 7) // 8)
    else:
        prefix = dimension
        candidates = [item[0] for item in sorted(vectors, key=lambda item: (dense_distance(item[1], prefix), item[0]))]
        payload = prefix * 4 // 8
    candidate_count = min(len(candidates), int(alpha * k + 0.999999))
    candidates = candidates[:candidate_count]
    candidate_latency_ms = (perf_counter() - candidate_start) * 1000
    rescore_start = perf_counter()
    by_id = dict(vectors)
    rescored = [item[0] for item in sorted(((item, dense_distance(by_id[item])) for item in candidates), key=lambda pair: (pair[1], pair[0]))][:k]
    rescore_latency_ms = (perf_counter() - rescore_start) * 1000
    recall = len(set(baseline) & set(candidates)) / len(baseline)
    quality_delta = 1 - sum(a == b for a, b in zip(baseline, rescored)) / len(baseline)
    dense_payload = 768 * 4 * candidate_count
    return {
        "candidate_recall_at_k": recall, "alpha": alpha,
        "candidate_latency_ms": candidate_latency_ms,
        "dense_rescore_latency_ms": rescore_latency_ms,
        "candidate_payload_bytes": payload * candidate_count,
        "dense_payload_bytes": dense_payload,
        "rss_theoretical_bytes": payload * candidate_count,
        "theoretical_payload_reduction": dense_payload / max(payload * candidate_count, 1),
        "quality_delta": quality_delta,
        "security_violations": 0,
        "freshness_violations": 0,
        "ranking_final": "dense_float32_rescore",
    }


dataset = load_json(args.dataset)
errors = validate_dataset(dataset)
out = run_dir(args.run_root, "benchmark")
manifest = base_manifest(out.name, canonical_hash(dataset), "benchmark-contract")
manifest["generation"]["seed"] = args.seed
manifest["concurrency"].update({"workers": args.concurrency, "read_update_ratio": args.read_update})
manifest["benchmark"] = {"warmup": args.warmup, "window": args.window, "restarts": args.restarts, "order": args.order, "bootstrap": {"method": "grouped", "samples": 10000, "confidence": 0.95}}
manifest["experimental_flags"] = {"CONTEXT_FABRIC_BQ_ENABLED": "shadow" if args.enable_bq else "off", "CONTEXT_FABRIC_MRL_ENABLED": "shadow" if args.enable_mrl else "off"}
manifest["stages"] = {stage: {"name": name, "enabled": stage not in ("A5", "A6") or (stage == "A5" and args.enable_bq) or (stage == "A6" and args.enable_mrl), "status": "synthetic-shadow-measured" if (stage == "A5" and args.enable_bq) or (stage == "A6" and args.enable_mrl) else "contract-only"} for stage, name in STAGES.items()}
metrics = {}
if args.enable_bq:
    metrics["bq"] = synthetic_shadow("bq")
if args.enable_mrl:
    metrics["mrl"] = synthetic_shadow("mrl")
manifest["status"] = "pending"
manifest["gate_results"] = {"security": "pending", "freshness": "pending", "bq": "off" if not args.enable_bq else "pending", "quality_loss": "pending", "compiler": "pending", "tool_search": "pending", "promotion": False, "fallback": "baseline"}
manifest["hashes"]["manifest_sha256"] = canonical_hash(manifest)
write_json(out / "manifest.json", manifest)
write_json(out / "results.json", {"schema": "NX-Gold v0", "status": "pending", "stage_metrics": manifest["stages"], "metrics": metrics, "missing": errors or ["real NX-Gold v0 dataset/evidence and full A0-A6 protocol not loaded"], "promotion": False, "fallback": "baseline"})
write_json(out / "gates.json", manifest["gate_results"])
print(out)
