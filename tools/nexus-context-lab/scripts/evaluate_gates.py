#!/usr/bin/env python3
import argparse, json, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from nxgold import evaluate_gates, load_json, write_json

parser = argparse.ArgumentParser(description="Fail-closed NX-Gold gate evaluator")
parser.add_argument("metrics", type=Path)
parser.add_argument("--baseline", type=Path, required=True)
parser.add_argument("--output", type=Path)
args = parser.parse_args()
result = evaluate_gates(load_json(args.metrics), load_json(args.baseline))
if args.output:
    write_json(args.output, result)
print(json.dumps(result, indent=2, sort_keys=True))
raise SystemExit(0 if result.get("promotion") else 1)
