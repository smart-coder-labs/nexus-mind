#!/usr/bin/env python3
import argparse, json, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from nxgold import base_manifest, canonical_hash, load_json, run_dir, validate_dataset, write_json

parser = argparse.ArgumentParser(description="Create an offline NX-Gold run manifest")
parser.add_argument("--dataset", type=Path, required=True)
parser.add_argument("--run-root", type=Path, default=Path("runs"))
parser.add_argument("--profile", default="offline-contract")
args = parser.parse_args()
dataset = load_json(args.dataset)
errors = validate_dataset(dataset)
out = run_dir(args.run_root)
manifest = base_manifest(out.name, canonical_hash(dataset), args.profile)
manifest["dataset_status"] = "ready" if not errors else "pending"
manifest["status"] = "complete" if not errors else "pending"
manifest["gate_results"] = {"dataset_complete": not errors, "promotion": False, "fallback": "baseline"}
manifest["stage_metrics"] = {stage: {"status": "contract-only", "implemented": False} for stage in ("A0", "A1", "A2", "A3", "A4", "A5", "A6")}
manifest["hashes"]["manifest_sha256"] = canonical_hash(manifest)
write_json(out / "manifest.json", manifest)
write_json(out / "results.json", {"schema": "NX-Gold v0", "status": manifest["status"], "errors": errors, "promotion": False})
write_json(out / "gates.json", manifest["gate_results"])
print(out)
if errors:
    print("NX-Gold v0: PENDING; baseline fallback; no promotion")
    raise SystemExit(1)
