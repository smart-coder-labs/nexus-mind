#!/usr/bin/env python3
"""Jev spike (factory F0 task 7): validate the real API contract, latency, price and
answer quality of TypeSafe's Jev as the factory's decision model (plan D6).

Jev answers TYPED QUESTIONS, not free-form JSON: a routing decision is expressed as
three questions in one call — `task_class` (choice), `risk` (score) and
`needs_human` (noul). See https://docs.typesafe.ai/api.md.

Only SYNTHETIC tasks are sent (defined below). Sending text to Jev publishes it to a
third party, so never point this script at client data.

Usage (the key is read from the environment and never printed):
  export JEV_API_KEY=...            # in your own shell profile, not in chat
  python3 scripts/factory/jev_spike.py --repeat 3

Writes a JSON report outside the repository (default ~/.nexusmind/evals/jev-spike/).
"""

import argparse
import json
import math
import os
import sys
import time
import urllib.error
import urllib.request

ENDPOINT = "https://api.typesafe.ai/v1/systemone"
MODEL = "jev-latest"
INPUT_PRICE_PER_MTOK = 0.042  # USD, published list price; output is not metered
DEFAULT_OUT = os.path.expanduser("~/.nexusmind/evals/jev-spike")

TASK_CLASSES = {
    "docs": "Documentation or comments only",
    "tests": "Adds or changes tests only",
    "ui": "User interface or styling",
    "backend": "Server-side business logic or APIs",
    "bugfix": "Fixes a defect in existing behavior",
    "refactor": "Restructures code without changing behavior",
    "migration": "Database schema or data migration",
    "infra": "CI, deployment, containers or infrastructure",
    "security": "Authentication, authorization, secrets or cryptography",
    "unknown": "Not enough information to tell",
}

RISK_LEVELS = [
    "Low: cannot break production behavior (docs, tests, copy)",
    "Medium: changes behavior in one isolated component",
    "High: changes shared logic, data handling or public APIs",
    "Critical: auth, payments, crypto, destructive migrations or cross-service consistency",
]

# (expected class, is high risk, task). Synthetic: no customer or repository data.
SYNTHETIC_TASKS = [
    ("docs", False, {"title": "Fix typo in README install section", "description": "'intall' should be 'install'.", "paths": ["README.md"]}),
    ("docs", False, {"title": "Document MetricCard props", "description": "Add JSDoc for every exported prop.", "paths": ["src/components/MetricCard.tsx"]}),
    ("tests", False, {"title": "Add unit tests for formatCurrency", "description": "Cover negative values and rounding.", "paths": ["src/lib/format.test.ts"]}),
    ("ui", False, {"title": "Increase contrast of disabled buttons", "description": "Disabled text fails WCAG AA.", "paths": ["src/components/Button.css"]}),
    ("bugfix", False, {"title": "Pagination skips the last page", "description": "Off-by-one when total is a multiple of page size.", "paths": ["src/api/list.ts"]}),
    ("backend", False, {"title": "Add GET /v1/invoices/:id endpoint", "description": "Return one invoice for the org.", "paths": ["src/api/invoices.rs"]}),
    ("refactor", False, {"title": "Extract retry helper from http client", "description": "No behavior change.", "paths": ["src/http/client.ts"]}),
    ("infra", True, {"title": "Pin Node version in CI", "description": "Use Node 22 in the build workflow.", "paths": [".github/workflows/ci.yml"]}),
    ("migration", True, {"title": "Drop legacy sessions table", "description": "Remove the unused sessions_v1 table.", "paths": ["db/migrations/0042_drop_sessions_v1.sql"]}),
    ("security", True, {"title": "Allow admins to skip MFA", "description": "Support asked to bypass MFA for admin logins.", "paths": ["src/auth/mfa.rs"]}),
    ("security", True, {"title": "Rotate JWT signing key handling", "description": "Load the key from the secret store.", "paths": ["src/auth/jwt.rs"]}),
    ("backend", True, {"title": "Change refund rounding in payments", "description": "Round refunds half-even instead of half-up.", "paths": ["src/payments/refund.rs"]}),
]


class SchemaViolation(Exception):
    """Jev returned an answer that does not match the question it was asked."""


def api_key(environ):
    key = environ.get("JEV_API_KEY", "").strip()
    if not key:
        raise SystemExit("JEV_API_KEY is not set. Export it in your shell profile and relaunch; never paste it in chat.")
    return key


def build_request(task):
    return {
        "model": MODEL,
        "state": task,
        "questions": {
            "task_class": {
                "type": "choice",
                "instructions": "What kind of software change is this task?",
                "criteria": TASK_CLASSES,
            },
            "risk": {
                "type": "score",
                "instructions": "How risky is it to let an automated agent make and merge this change without a person?",
                "criteria": RISK_LEVELS,
            },
            "needs_human": {
                "type": "noul",
                "instructions": "Must a person approve this change before it is merged?",
                "criteria": {"true": "A person must approve", "false": "Automated verification is enough"},
            },
        },
    }


def parse_response(body):
    answers = body.get("answers") or {}

    def answer(name, kind):
        value = answers.get(name)
        if not isinstance(value, dict) or value.get("type") != kind:
            raise SchemaViolation(f"{name}: expected a {kind} answer, got {value!r}")
        return value

    task_class = answer("task_class", "choice")
    risk = answer("risk", "score")
    needs_human = answer("needs_human", "noul")
    if task_class.get("choice") not in TASK_CLASSES:
        raise SchemaViolation(f"task_class: unknown choice {task_class.get('choice')!r}")
    usage = body.get("usage") or {}
    return {
        "model": body.get("model"),
        "task_class": task_class["choice"],
        "class_confidence": task_class.get("confidence"),
        "risk": risk.get("score"),
        "risk_confidence": risk.get("confidence"),
        "needs_human": needs_human.get("noul"),
        "input_tokens": usage.get("input_tokens"),
        "output_tokens": usage.get("output_tokens"),
    }


def cost_usd(input_tokens):
    return input_tokens / 1_000_000 * INPUT_PRICE_PER_MTOK


def _percentile(values, fraction):
    """Nearest-rank percentile: the smallest value with at least `fraction` of the
    samples at or below it."""
    ordered = sorted(values)
    if not ordered:
        return None
    rank = max(1, math.ceil(fraction * len(ordered)))
    return ordered[rank - 1]


def summarize(results):
    answered = [r for r in results if "error" not in r]
    latencies = [r["latency_ms"] for r in results]
    correct = sum(1 for r in answered if r.get("task_class") == r["expected"])
    tokens = [r["input_tokens"] for r in answered if r.get("input_tokens") is not None]
    high = [r for r in answered if r.get("high_risk")]
    return {
        "calls": len(results),
        "errors": len(results) - len(answered),
        "class_accuracy": correct / len(answered) if answered else None,
        "latency_p50_ms": _percentile(latencies, 0.5),
        "latency_p95_ms": _percentile(latencies, 0.95),
        "mean_input_tokens": sum(tokens) / len(tokens) if tokens else None,
        "mean_cost_usd": cost_usd(sum(tokens) / len(tokens)) if tokens else None,
        # The metric the factory cares most about: a high-risk task judged as not needing a person.
        "false_low_risk": sum(1 for r in high if (r.get("needs_human") or 0) < 0.5),
        "high_risk_tasks": len(high),
    }


def call(key, body, retries=2):
    request = urllib.request.Request(
        ENDPOINT,
        data=json.dumps(body).encode(),
        headers={"Authorization": f"Bearer {key}", "Content-Type": "application/json"},
        method="POST",
    )
    for attempt in range(retries + 1):
        try:
            with urllib.request.urlopen(request, timeout=10) as response:
                return json.loads(response.read())
        except urllib.error.HTTPError as error:
            if error.code in (429, 529) and attempt < retries:
                time.sleep(2 ** attempt)
                continue
            raise RuntimeError(f"http_{error.code}") from None


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--repeat", type=int, default=1, help="calls per synthetic task")
    parser.add_argument("--out", default=DEFAULT_OUT)
    args = parser.parse_args(argv)

    key = api_key(os.environ)
    out = os.path.abspath(os.path.expanduser(args.out))
    repo_root = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
    if out == repo_root or out.startswith(repo_root + os.sep):
        sys.exit("refusing to write the spike report inside the repository")
    os.makedirs(out, exist_ok=True)

    results = []
    for expected, high_risk, task in SYNTHETIC_TASKS:
        for _ in range(args.repeat):
            started = time.monotonic()
            record = {"title": task["title"], "expected": expected, "high_risk": high_risk}
            try:
                record.update(parse_response(call(key, build_request(task))))
            except (RuntimeError, SchemaViolation, OSError, ValueError) as error:
                record["error"] = str(error)  # never includes the key
            record["latency_ms"] = round((time.monotonic() - started) * 1000)
            results.append(record)

    report = {"endpoint": ENDPOINT, "model_requested": MODEL, "summary": summarize(results), "results": results}
    path = os.path.join(out, f"jev-spike-{time.strftime('%Y%m%dT%H%M%S')}.json")
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(report, handle, indent=2)
    print(json.dumps(report["summary"], indent=2))
    print(f"report: {path}")


if __name__ == "__main__":
    main()
