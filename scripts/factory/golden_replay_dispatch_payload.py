#!/usr/bin/env python3
"""Write the safe workflow_dispatch payload for one frozen golden JSONL file."""

import json
import re
import sys
import uuid
from pathlib import Path

ALLOWED_REPOSITORIES = {"smart-coder-labs/nexus-mind", "kasymir/kasymir-app-ui"}
ALLOWED_KEYS = {"id", "repository", "merge_sha", "changed_files"}
MAX_MANIFEST_BYTES = 40_000


def main() -> None:
    if len(sys.argv) != 3:
        raise SystemExit("usage: golden_replay_dispatch_payload.py <dataset.jsonl> <org_id>")
    dataset_path = Path(sys.argv[1])
    org_id = str(uuid.UUID(sys.argv[2]))
    rows = []
    seen = set()
    for line_number, line in enumerate(dataset_path.read_text(encoding="utf-8").splitlines(), 1):
        if not line.strip():
            continue
        try:
            task = json.loads(line)
        except json.JSONDecodeError as error:
            raise SystemExit(f"invalid JSON on line {line_number}") from error
        if not isinstance(task, dict) or not ALLOWED_KEYS.issubset(task):
            raise SystemExit(f"line {line_number} is missing replay fields")
        task_id = str(uuid.UUID(task["id"]))
        if task_id != task["id"] or task_id in seen:
            raise SystemExit(f"line {line_number} has a non-canonical or duplicate id")
        seen.add(task_id)
        if task["repository"] not in ALLOWED_REPOSITORIES:
            raise SystemExit(f"line {line_number} repository is not allowed")
        merge_sha = task["merge_sha"]
        if not isinstance(merge_sha, str) or not re.fullmatch(r"[0-9a-fA-F]{40}", merge_sha):
            raise SystemExit(f"line {line_number} has an invalid merge_sha")
        paths = task["changed_files"]
        if not isinstance(paths, list) or len(paths) > 40:
            raise SystemExit(f"line {line_number} changed_files must contain at most 40 paths")
        if any(
            not isinstance(path, str)
            or not path
            or path.startswith("/")
            or "\\" in path
            or "\x00" in path
            or ".." in path.split("/")
            for path in paths
        ):
            raise SystemExit(f"line {line_number} has an invalid changed_files path")
        # Drop task text, titles, labels, base SHA and all other fields before
        # anything leaves the local frozen dataset directory.
        rows.append({
            "id": task_id,
            "repository": task["repository"],
            "merge_sha": merge_sha.lower(),
            "changed_files": paths,
        })
    if not rows or len(rows) > 50:
        raise SystemExit("each dispatch must contain between 1 and 50 tasks")
    tasks_jsonl = "".join(json.dumps(row, separators=(",", ":")) + "\n" for row in rows)
    if len(tasks_jsonl.encode("utf-8")) > MAX_MANIFEST_BYTES:
        raise SystemExit(f"sanitized manifest exceeds {MAX_MANIFEST_BYTES} bytes; split the dataset file")
    print(json.dumps({"action": "start", "org_id": org_id, "tasks_jsonl": tasks_jsonl}, separators=(",", ":")))


if __name__ == "__main__":
    main()
