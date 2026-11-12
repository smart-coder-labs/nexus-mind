#!/usr/bin/env python3
"""Create a supervised F1 replay Job from the live worker pod template."""

from __future__ import annotations

import copy
import json
import re
import subprocess
import sys
import uuid
from typing import Any

NAMESPACE = "nexusmind"
DEPLOYMENT = "nexusmind-backend"
WORKER_CONTAINER = "autonomous-worker"
TASKS_PATH = "/factory-input/tasks.jsonl"


def validate_inputs(replay_id: str, org_id: str, config_map: str) -> None:
    if not re.fullmatch(r"[0-9]{1,20}", replay_id):
        raise ValueError("invalid_replay_id")
    try:
        canonical_org_id = str(uuid.UUID(org_id))
    except (ValueError, AttributeError) as error:
        raise ValueError("invalid_org_id") from error
    if canonical_org_id != org_id.lower():
        raise ValueError("noncanonical_org_id")
    if config_map != f"f1-replay-{replay_id}":
        raise ValueError("invalid_config_map")


def build_job(
    deployment: dict[str, Any], replay_id: str, org_id: str, config_map: str
) -> dict[str, Any]:
    validate_inputs(replay_id, org_id, config_map)
    template = copy.deepcopy(deployment["spec"]["template"]["spec"])
    containers = template.get("containers", [])
    worker = next(
        (container for container in containers if container.get("name") == WORKER_CONTAINER),
        None,
    )
    if worker is None:
        raise ValueError("worker_container_missing")

    # This Job only coordinates one verification pod at a time. Keep its CPU
    # reservation small so that the 50m command pod can still be scheduled on
    # the single-node production cluster while the Job is running.
    resources = worker.setdefault("resources", {})
    requests = resources.setdefault("requests", {})
    requests["cpu"] = "50m"

    worker["command"] = ["/bin/sh", "-ec"]
    worker["args"] = [
        f"exec /app/factory-golden-replay {org_id} < {TASKS_PATH}"
    ]
    for probe in ("startupProbe", "livenessProbe", "readinessProbe"):
        worker.pop(probe, None)
    worker.pop("ports", None)
    worker.setdefault("volumeMounts", []).append(
        {"name": "factory-replay-input", "mountPath": "/factory-input", "readOnly": True}
    )

    volumes = template.setdefault("volumes", [])
    if any(volume.get("name") == "factory-replay-input" for volume in volumes):
        raise ValueError("replay_volume_already_exists")
    volumes.append(
        {
            "name": "factory-replay-input",
            "configMap": {
                "name": config_map,
                "items": [{"key": "tasks.jsonl", "path": "tasks.jsonl"}],
            },
        }
    )

    template["containers"] = [worker]
    template["restartPolicy"] = "Never"
    pod_template = {
        "metadata": {
            "labels": {
                "app.kubernetes.io/name": "factory-golden-replay",
                "factory-replay-id": replay_id,
            }
        },
        "spec": template,
    }
    return {
        "apiVersion": "batch/v1",
        "kind": "Job",
        "metadata": {
            "name": config_map,
            "namespace": NAMESPACE,
            "labels": {"app.kubernetes.io/name": "factory-golden-replay"},
        },
        "spec": {
            "backoffLimit": 0,
            "activeDeadlineSeconds": 21600,
            "ttlSecondsAfterFinished": 604800,
            "template": pod_template,
        },
    }


def kubectl(*args: str, input_text: str | None = None) -> str:
    result = subprocess.run(
        ["sudo", "kubectl", *args],
        input=input_text,
        text=True,
        capture_output=True,
        check=True,
    )
    return result.stdout


def main() -> int:
    if len(sys.argv) != 4:
        print("usage: create_golden_replay_job.py REPLAY_ID ORG_ID CONFIG_MAP", file=sys.stderr)
        return 2
    replay_id, org_id, config_map = sys.argv[1:]
    try:
        validate_inputs(replay_id, org_id, config_map)
        deployment = json.loads(
            kubectl("-n", NAMESPACE, "get", "deployment", DEPLOYMENT, "-o", "json")
        )
        job = build_job(deployment, replay_id, org_id, config_map)
        try:
            created = json.loads(
                kubectl("create", "-f", "-", "-o", "json", input_text=json.dumps(job))
            )
        except (json.JSONDecodeError, subprocess.CalledProcessError):
            try:
                kubectl("-n", NAMESPACE, "delete", "configmap", config_map, "--ignore-not-found")
            except subprocess.CalledProcessError:
                pass
            raise
        owner = {
            "apiVersion": created["apiVersion"],
            "kind": created["kind"],
            "name": created["metadata"]["name"],
            "uid": created["metadata"]["uid"],
            "controller": False,
            "blockOwnerDeletion": False,
        }
        try:
            kubectl(
                "-n",
                NAMESPACE,
                "patch",
                "configmap",
                config_map,
                "--type=merge",
                "--patch",
                json.dumps({"metadata": {"ownerReferences": [owner]}}),
            )
        except subprocess.CalledProcessError:
            # The job already has its input mounted; only TTL-based cleanup is lost.
            print("factory_replay_input_ttl_cleanup_not_configured", file=sys.stderr)
    except (ValueError, KeyError, json.JSONDecodeError, subprocess.CalledProcessError) as error:
        print(f"factory_replay_job_create_failed: {error}", file=sys.stderr)
        return 1
    print(config_map)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
