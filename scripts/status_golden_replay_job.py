#!/usr/bin/env python3
"""Read a supervised F1 replay Job without exposing its task output."""

from __future__ import annotations

import json
import re
import subprocess
import sys
from typing import Any

NAMESPACE = "nexusmind"
DEPLOYMENT = "nexusmind-backend"
WORKER_CONTAINER = "autonomous-worker"


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


def progress_from_logs(logs: str) -> dict[str, int]:
    """Aggregate per-task outcomes without returning IDs, SHAs or command output."""
    counts = {"completed": 0, "passed": 0, "failed": 0, "skipped_no_evidence": 0, "errors": 0}
    for line in logs.splitlines():
        try:
            value: Any = json.loads(line)
        except json.JSONDecodeError:
            continue
        if not isinstance(value, dict) or not isinstance(value.get("id"), str):
            continue
        if not isinstance(value.get("passed"), bool) or not isinstance(value.get("skipped"), bool):
            continue
        counts["completed"] += 1
        if value.get("error") is not None:
            counts["errors"] += 1
        elif value["skipped"]:
            counts["skipped_no_evidence"] += 1
        elif value["passed"]:
            counts["passed"] += 1
        else:
            counts["failed"] += 1
    return counts


def legacy_result(process_state: str, logs: str) -> dict[str, Any]:
    """Summarize runs started by the pre-Job runner without exposing task logs."""
    if process_state == "not_found":
        return {"state": "not_found"}
    if process_state == "running":
        return {"state": "running"}
    if process_state != "finished":
        return {"state": "status_unavailable"}
    summary = summary_from_logs(logs)
    if summary is None:
        return {"state": "incomplete"}
    return {"state": "complete", "summary": summary}


def legacy_status(replay_id: str) -> dict[str, Any]:
    # IDs are validated as decimal before reaching this function. The shell
    # checks only the legacy worker's PID and prints no task output.
    state_script = (
        'dir="/data/factory-evals/f1-replay-$1"; '
        'if [ ! -f "$dir/pid" ]; then echo not_found; exit 0; fi; '
        'pid=$(cat "$dir/pid"); '
        'if [ -n "$pid" ] && [ -r "/proc/$pid/cmdline" ] && '
        'tr "\\000" " " < "/proc/$pid/cmdline" | grep -q "factory-golden-replay"; '
        'then echo running; else echo finished; fi'
    )
    command = [
        "-n", NAMESPACE, "exec", f"deploy/{DEPLOYMENT}", "-c", WORKER_CONTAINER,
        "--", "sh", "-c", state_script, "sh", replay_id,
    ]
    state_result = kubectl(*command)
    if state_result.returncode != 0:
        return {"state": "status_unavailable"}
    process_state = state_result.stdout.strip().splitlines()[-1:] or ["unknown"]
    state = process_state[0]
    if state != "finished":
        return legacy_result(state, "")

    output_result = kubectl(
        "-n", NAMESPACE, "exec", f"deploy/{DEPLOYMENT}", "-c", WORKER_CONTAINER,
        "--", "cat", f"/data/factory-evals/f1-replay-{replay_id}/out.jsonl",
    )
    logs = output_result.stdout if output_result.returncode == 0 else ""
    return legacy_result("finished", logs)


def main() -> int:
    if len(sys.argv) != 2 or not re.fullmatch(r"[0-9]{1,20}", sys.argv[1]):
        print('usage: status_golden_replay_job.py REPLAY_ID', file=sys.stderr)
        return 2
    name = f"f1-replay-{sys.argv[1]}"
    result = kubectl("-n", NAMESPACE, "get", "job", name, "-o", "json")
    if result.returncode != 0:
        if "NotFound" not in result.stderr:
            print(json.dumps({"state": "status_unavailable"}, separators=(",", ":")))
            return 1
        status = legacy_status(sys.argv[1])
        print(json.dumps(status, separators=(",", ":")))
        return 0 if status["state"] == "running" else int(status["state"] != "complete")
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

    logs = kubectl("-n", NAMESPACE, "logs", f"job/{name}")
    progress = progress_from_logs(logs.stdout) if logs.returncode == 0 else None
    result = {"state": "running"}
    if progress and progress["completed"]:
        result["progress"] = progress
    print(json.dumps(result, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
