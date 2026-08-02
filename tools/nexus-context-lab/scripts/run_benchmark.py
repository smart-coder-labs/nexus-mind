#!/usr/bin/env python3
import argparse, sys
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
dataset = load_json(args.dataset)
errors = validate_dataset(dataset)
out = run_dir(args.run_root, "benchmark")
manifest = base_manifest(out.name, canonical_hash(dataset), "benchmark-contract")
manifest["generation"]["seed"] = args.seed
manifest["concurrency"].update({"workers": args.concurrency, "read_update_ratio": args.read_update})
manifest["benchmark"] = {"warmup": args.warmup, "window": args.window, "restarts": args.restarts, "order": args.order, "bootstrap": {"method": "grouped", "samples": 10000, "confidence": 0.95}}
manifest["stages"] = {stage: {"name": name, "enabled": stage not in ("A5", "A6") or (stage == "A5" and args.enable_bq) or (stage == "A6" and args.enable_mrl), "status": "contract-only"} for stage, name in STAGES.items()}
manifest["status"] = "pending"
manifest["gate_results"] = {"security": "pending", "freshness": "pending", "bq": "off" if not args.enable_bq else "pending", "quality_loss": "pending", "compiler": "pending", "tool_search": "pending", "promotion": False, "fallback": "baseline"}
manifest["hashes"]["manifest_sha256"] = canonical_hash(manifest)
write_json(out / "manifest.json", manifest)
write_json(out / "results.json", {"schema": "NX-Gold v0", "status": "pending", "stage_metrics": manifest["stages"], "metrics": {}, "missing": errors or ["real stage implementations and measurements not loaded"], "promotion": False})
write_json(out / "gates.json", manifest["gate_results"])
print(out)
