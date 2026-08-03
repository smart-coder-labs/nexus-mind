#!/usr/bin/env python3
import argparse, json, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from nxgold import load_json, validate_promotion, validate_structure

parser = argparse.ArgumentParser(description="Validate NX-Gold v0 without network")
parser.add_argument("dataset", type=Path)
parser.add_argument("--mode", choices=("structural", "promotion"), default="structural")
args = parser.parse_args()
dataset = load_json(args.dataset)
errors = validate_structure(dataset) if args.mode == "structural" else validate_promotion(dataset)
if errors:
    print(f"NX-Gold v0 {args.mode}: PENDING/REJECTED")
    for error in errors:
        print(f"- {error}")
    raise SystemExit(1)
print(f"NX-Gold v0 {args.mode}: READY")
