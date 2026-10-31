#!/usr/bin/env python3
"""Read a supervised F1 replay Job without exposing its task output."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from typing import Any

NAMESPACE = "nexusmind"


def kubectl(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        ["sudo", "kubectl", *args], text=True, capture_output=True, check=False
    )


def summary_from_logs(logs: str) -> dict[str, int] | None:
    for line in reversed(logs.splitlines()):
        try:
            value: Any = json.loads(line)
        except json.JSONDecodeError:
            continue
        summary = value.get("summary") if isinstance(value, dict) else None
        if isinstance(summary, dict) and all(
            isinstance(summary.get(key), int)
            for key in ("tasks", "passed", "failed", "skipped_no_evidence", "errors")
        ):
            return {
                key: summary[key]
                for key in ("tasks", "passed", "failed", "skipped_no_evidence", "errors")
            }
    return None


def main() -> int:
    if len(sys.argv) != 2 or not re.fullmatch(r"[0-9]{1,20}", sys.argv[1]):
        print('usage: status_golden_replay_job.py REPLAY_ID', file=sys.stderr)
        return 2
    name = f"f1-replay-{sys.argv[1]}"
    result = kubectl("-n", NAMESPACE, "get", "job", name, "-o", "json")
    if result.returncode != 0:
        print(json.dumps({"state": "not_found"}, separators=(",", ":")))
        return 1
    try:
        job = json.loads(result.stdout)
    except json.JSONDecodeError:
        print(json.dumps({"state": "status_unavailable"}, separators=(",", ":")))
        return 1

    conditions = job.get("status", {}).get("conditions", [])
    if any(condition.get("type") == "Complete" and condition.get("status") == "True" for condition in conditions):
        logs = kubectl("-n", NAMESPACE, "logs", f"job/{name}")
        summary = summary_from_logs(logs.stdout) if logs.returncode == 0 else None
        if summary is None:
            print(json.dumps({"state": "incomplete"}, separators=(",", ":")))
            return 1
        print(json.dumps({"state": "complete", "summary": summary}, separators=(",", ":")))
        return 0

    if any(condition.get("type") == "Failed" and condition.get("status") == "True" for condition in conditions):
        print(json.dumps({"state": "failed"}, separators=(",", ":")))
        return 1

    print(json.dumps({"state": "running"}, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
