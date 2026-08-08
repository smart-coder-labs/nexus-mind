#!/usr/bin/env python3
"""Generate the complete, deterministic, offline NX-Gold v0 sample fixture."""
from __future__ import annotations

import argparse
import sys
from pathlib import Path
from typing import Dict, Iterable, List

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from nxgold import (  # noqa: E402
    ANSWERABILITY,
    EVALUATIONS,
    LANGUAGES,
    PLANES,
    SPLITS,
    SCHEMA,
    canonical_hash,
    validate_structure,
    write_json,
)


TENANTS = ("tenant-a", "tenant-b", "tenant-c")
ANNOTATORS = ("annotator-alpha", "annotator-beta")


def _distribution(values: Dict[str, int], seed: int) -> List[str]:
    scale = 300 // sum(values.values()) if sum(values.values()) < 300 else 1
    items = [value for value, count in values.items() for _ in range(count * scale)]
    offset = seed % len(items)
    return items[offset:] + items[:offset]


def _other_tenants(tenant: str) -> List[str]:
    return [candidate for candidate in TENANTS if candidate != tenant]


def generate_fixture(seed: int = 20260803) -> dict:
    fields = (
        _distribution(PLANES, seed),
        _distribution(LANGUAGES, seed + 1),
        _distribution(ANSWERABILITY, seed + 2),
        _distribution(EVALUATIONS, seed + 3),
        _distribution(SPLITS, seed + 4),
    )
    scenarios: List[dict] = []
    executions: List[dict] = []
    adjudications: List[dict] = []
    for index in range(300):
        number = index + 1
        scenario_id = f"scenario-{number:03d}"
        tenant = TENANTS[index % len(TENANTS)]
        document_id = f"{tenant}-document-{number:03d}"
        chunk_id = f"{document_id}-chunk-01"
        required = [document_id, chunk_id]
        prohibited = [f"{other}-document-{number:03d}" for other in _other_tenants(tenant)]
        answerable = fields[2][index] == "answerable"
        scenario = {
            "id": scenario_id,
            "tenant": tenant,
            "plane": fields[0][index],
            "language": fields[1][index],
            "answerability": fields[2][index],
            "evaluation": fields[3][index],
            "split": fields[4][index],
            "query": f"Synthetic query {number:03d} for {tenant}",
            "negatives": [
                f"{tenant}-negative-{number:03d}-{negative:02d}"
                for negative in range(1, 4 + (index % 3))
            ],
            "gold": {
                "answer": f"Synthetic answer {number:03d}" if answerable else None,
                "required_evidence": required,
                "prohibited_evidence": prohibited,
            },
            "locator": {"document_id": document_id, "chunk_id": chunk_id, "span": [0, 32]},
            "relevance": {"graded": 3 if answerable else 0, "threshold": 2},
            "tools": {"allowed": ["retrieval", "context"], "expected": ["retrieval"]},
            "abstention": {"allowed": not answerable, "required": not answerable},
        }
        scenarios.append(scenario)
        for execution_number in range(1, 4):
            executions.append({
                "id": f"{scenario_id}-execution-{execution_number}",
                "scenario_id": scenario_id,
                "execution": execution_number,
                "tenant": tenant,
                "split": scenario["split"],
                "seed": seed + number * 10 + execution_number,
                "status": "sample",
            })
        adjudications.append({
            "scenario_id": scenario_id,
            "annotator_a": ANNOTATORS[0],
            "annotator_b": ANNOTATORS[1],
            "decision": "agree" if index % 4 else "adjudicated",
            "audit": f"adjudication-{number:03d}",
        })

    fixture = {
        "schema": SCHEMA,
        "status": "sample",
        "synthetic_corpus": True,
        "metadata": {
            "synthetic_corpus": True,
            "generator": "generate_nx_gold_fixture.py",
            "seed": seed,
            "promotion_note": "Structural contract fixture only; never NX-Gold promotion evidence.",
        },
        "scenarios": scenarios,
        "executions": executions,
        "annotations": {"annotators": list(ANNOTATORS), "adjudications": adjudications},
        "canaries": [
            {"id": f"{tenant}-canary", "tenant": tenant, "content": f"SYNTHETIC_CANARY_{tenant[-1].upper()}"}
            for tenant in TENANTS
        ],
        "evidence_contract": {
            "required": ["required_evidence", "prohibited_evidence", "locator", "relevance", "tools", "abstention"],
            "required_evidence": "Gold documents and chunks that must support an answer.",
            "prohibited_evidence": "Documents and chunks from other tenants or marked negative.",
            "locator": "Stable document, chunk, and span locator.",
            "relevance": "Ordinal relevance grade and acceptance threshold.",
            "tools": "Allowed tools and expected retrieval tool calls.",
            "abstention": "Whether abstention is allowed and required.",
        },
    }
    fixture["fixture_sha256"] = canonical_hash(fixture)
    errors = validate_structure(fixture)
    if errors:
        raise ValueError("generated fixture is invalid: " + "; ".join(errors))
    return fixture


def generate(output: Path, seed: int = 20260803) -> dict:
    fixture = generate_fixture(seed)
    write_json(output, fixture)
    return fixture


def main() -> int:
    parser = argparse.ArgumentParser(description="Generate a deterministic synthetic NX-Gold v0 fixture")
    parser.add_argument("--output", type=Path, default=Path("fixtures/nx_gold_synthetic_fixture.json"))
    parser.add_argument("--seed", type=int, default=20260803)
    args = parser.parse_args()
    fixture = generate(args.output, args.seed)
    print(f"{args.output} {fixture['fixture_sha256']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
