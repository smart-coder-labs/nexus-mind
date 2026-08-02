#!/usr/bin/env python3
import argparse, json, sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from nxgold import load_json, validate_dataset

parser = argparse.ArgumentParser(description="Validate NX-Gold v0 without network")
parser.add_argument("dataset", type=Path)
args = parser.parse_args()
errors = validate_dataset(load_json(args.dataset))
if errors:
    print("NX-Gold v0: PENDING/REJECTED")
    for error in errors:
        print(f"- {error}")
    raise SystemExit(1)
print("NX-Gold v0: READY")
