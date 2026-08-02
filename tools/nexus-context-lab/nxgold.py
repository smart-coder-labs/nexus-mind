"""Offline NX-Gold v0 contracts. No network, subprocesses, or product data."""
from __future__ import annotations

import hashlib
import json
import os
import platform
import re
import shutil
import sys
import time
from pathlib import Path
from typing import Any, Dict, Iterable, List

SCHEMA = "NX-Gold v0"
SNAPSHOTS = {"nexusmind": "cf0378f...", "mcp": "d2fe..."}
PLANES = {"memory": 75, "code": 75, "sdd": 60, "policy": 45, "conversation": 45}
LANGUAGES = {"es": 45, "en": 35, "pt": 20}
ANSWERABILITY = {"answerable": 80, "unanswerable": 20}
EVALUATIONS = {"exact": 35, "semantic": 40, "multihop": 25}
SPLITS = {"train": 60, "dev": 20, "test": 20}
STAGES = {f"A{i}": name for i, name in enumerate(("fts5", "dense_float32", "hybrid_rrf", "policy_first_generations", "compiler", "bq_shadow", "mrl"))}


class ValidationError(ValueError):
    pass


def load_json(path: Path) -> Dict[str, Any]:
    with path.open(encoding="utf-8") as handle:
        value = json.load(handle)
    if not isinstance(value, dict):
        raise ValidationError("dataset root must be an object")
    return value


def _counts(values: Iterable[Any]) -> Dict[Any, int]:
    result: Dict[Any, int] = {}
    for value in values:
        result[value] = result.get(value, 0) + 1
    return result


def validate_dataset(dataset: Dict[str, Any]) -> List[str]:
    errors: List[str] = []
    if dataset.get("schema") != SCHEMA:
        errors.append("schema must be NX-Gold v0")
    scenarios = dataset.get("scenarios")
    executions = dataset.get("executions")
    if not isinstance(scenarios, list) or len(scenarios) != 300:
        errors.append("exactly 300 scenarios are required")
    if not isinstance(executions, list) or len(executions) != 900:
        errors.append("exactly 900 executions are required")
    if not isinstance(scenarios, list):
        return errors
    ids = [item.get("id") if isinstance(item, dict) else None for item in scenarios]
    if None in ids or len(set(ids)) != len(ids):
        errors.append("scenario ids must be unique")
    for field, expected in (("plane", PLANES), ("language", LANGUAGES), ("answerability", ANSWERABILITY), ("evaluation", EVALUATIONS), ("split", SPLITS)):
        actual = _counts(item.get(field) for item in scenarios if isinstance(item, dict))
        if actual != expected:
            errors.append(f"{field} distribution must be {expected}, got {actual}")
    scenario_ids = set(ids)
    for item in scenarios:
        if not isinstance(item, dict):
            errors.append("each scenario must be an object")
            continue
        negatives = item.get("negatives")
        if not isinstance(negatives, list) or not 3 <= len(negatives) <= 5:
            errors.append(f"{item.get('id')}: negatives must contain 3-5 items")
        required = item.get("gold", {}).get("required_evidence") if isinstance(item.get("gold"), dict) else None
        prohibited = item.get("gold", {}).get("prohibited_evidence") if isinstance(item.get("gold"), dict) else None
        missing = [field for field in ("gold", "locator", "relevance", "tools", "abstention") if field not in item]
        if missing:
            errors.append(f"{item.get('id')}: missing evidence contract fields {missing}")
        if not isinstance(required, list) or not isinstance(prohibited, list):
            errors.append(f"{item.get('id')}: required/prohibited evidence lists are required")
        elif set(required) & set(prohibited):
            errors.append(f"{item.get('id')}: required and prohibited evidence overlap")
        if isinstance(item.get("gold"), dict) and isinstance(item["gold"].get("answer"), str):
            answer = item["gold"]["answer"]
            if "SYNTHETIC_CANARY_" in answer and item.get("tenant") not in answer:
                errors.append(f"{item.get('id')}: canary leakage in answer")
    if isinstance(executions, list):
        by_scenario = _counts(item.get("scenario_id") for item in executions if isinstance(item, dict))
        if set(by_scenario) != scenario_ids or any(count != 3 for count in by_scenario.values()):
            errors.append("each scenario must have exactly three executions")
    annotations = dataset.get("annotations")
    if not isinstance(annotations, dict) or len(annotations.get("annotators", [])) != 2 or len(annotations.get("adjudications", [])) < 1:
        errors.append("two annotators and adjudication records are required")
    canaries = dataset.get("canaries")
    tenants = {item.get("tenant") for item in scenarios if isinstance(item, dict)}
    canary_tenants = {item.get("tenant") for item in canaries} if isinstance(canaries, list) else set()
    if not isinstance(canaries, list) or len(canaries) != 3 or canary_tenants != {"tenant-a", "tenant-b", "tenant-c"}:
        errors.append("exactly three fictitious tenant canaries are required")
    if tenants and not tenants.issubset(canary_tenants):
        errors.append("scenario tenants must be fictitious canary tenants")
    contract = dataset.get("evidence_contract")
    required_contract = {"required_evidence", "prohibited_evidence", "locator", "relevance", "tools", "abstention"}
    if not isinstance(contract, dict) or not required_contract.issubset(set(contract.get("required", []))):
        errors.append("evidence_contract is incomplete")
    if dataset.get("status") != "complete":
        errors.append("dataset status is sample/pending; real NX-Gold v0 must be complete")
    return errors


def canonical_hash(value: Any) -> str:
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode()
    return hashlib.sha256(encoded).hexdigest()


def _ram_bytes() -> int:
    if sys.platform == "darwin":
        try:
            import subprocess
            return int(subprocess.check_output(["sysctl", "-n", "hw.memsize"], text=True).strip())
        except Exception:
            return 0
    return int(os.sysconf("SC_PAGE_SIZE") * os.sysconf("SC_PHYS_PAGES")) if hasattr(os, "sysconf") else 0


def preflight(env: Dict[str, str], cwd: Path) -> List[str]:
    errors: List[str] = []
    workspace_raw = env.get("NEXUS_LAB_WORKSPACE", "")
    workspace = Path(workspace_raw).expanduser() if workspace_raw else None
    forbidden = ("nexus-local-qa", "production", "productivo", "prod", "nexusmind-data")
    if workspace is None or not workspace.is_absolute() or not workspace.exists() or not workspace.is_dir():
        errors.append("NEXUS_LAB_WORKSPACE must be an existing absolute directory")
    elif any(part.lower() in forbidden for part in workspace.parts):
        errors.append("workspace path is forbidden (nexus-local-qa/productive path)")
    if workspace and workspace.exists() and workspace.resolve() == cwd.resolve():
        errors.append("lab workspace must be separate from the repository checkout")
    url = env.get("LOOPBACK_URL", "")
    if not (url.startswith("http://127.0.0.1:") or url.startswith("http://localhost:")):
        errors.append("LOOPBACK_URL must be an http loopback URL")
    allowlist = [x.strip() for x in env.get("CORS_ALLOWLIST", "").split(",") if x.strip()]
    if not allowlist or any(not (x.startswith("http://127.0.0.1:") or x.startswith("http://localhost:")) for x in allowlist):
        errors.append("CORS_ALLOWLIST must contain only declarative loopback origins")
    if not env.get("FAKE_API_KEY", "").startswith("fake-") or not env.get("FAKE_DATABASE_URL", "").startswith("sqlite:///"):
        errors.append("credentials must be explicitly fake and local sqlite")
    model = Path(env.get("MODEL_PATH", ""))
    model_hash = env.get("MODEL_SHA256", "")
    if env.get("MODEL_PREFETCHED") != "true" or not re.fullmatch(r"[0-9a-fA-F]{64}", model_hash):
        errors.append("prefetched model and registered hash are required")
    if not model.is_absolute() or not model.is_file():
        errors.append("MODEL_PATH must be absolute")
    if env.get("EGRESS_ENABLED", "true").lower() != "false":
        errors.append("EGRESS_ENABLED must be false")
    try:
        free = shutil.disk_usage(workspace or cwd).free
        if free < int(env.get("MIN_FREE_BYTES", "0")):
            errors.append("insufficient free disk")
    except (ValueError, OSError):
        errors.append("invalid disk resource threshold")
    try:
        if _ram_bytes() < int(env.get("MIN_RAM_BYTES", "0")):
            errors.append("insufficient RAM")
        if (os.cpu_count() or 0) < int(env.get("MIN_CPU_COUNT", "0")):
            errors.append("insufficient CPU")
    except ValueError:
        errors.append("invalid RAM/CPU resource threshold")
    return errors


def base_manifest(run_id: str, dataset_hash: str, profile: str = "offline-contract", model: str = "none") -> Dict[str, Any]:
    return {
        "schema": SCHEMA, "harness": "NX-Gold", "run_id": run_id, "status": "pending",
        "snapshots": SNAPSHOTS, "profile": profile,
        "model": {"name": model, "prefetched": False, "sha256": "not-loaded"},
        "generation": {"temperature": 0, "seed": 20260802, "max_tokens": 512},
        "hardware": {"platform": platform.platform(), "python": platform.python_version(), "cpu_count": os.cpu_count()},
        "concurrency": {"workers": 1, "protocol": "offline-file", "read_update_ratio": "80/20"},
        "isolation_checks": {"network": "not-run", "docker": "not-run", "product_data": "not-run", "tenant_canary": "not-run"},
        "hashes": {"dataset_sha256": dataset_hash, "manifest_input_sha256": canonical_hash({"dataset": dataset_hash, "seed": 20260802})},
        "stage_metrics": {}, "gate_results": {}, "dataset_status": "pending",
    }


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def run_dir(run_root: Path, prefix: str = "nxgold") -> Path:
    # The timestamp is only a directory label; all measured inputs remain hashed.
    return run_root / f"{prefix}-{time.strftime('%Y%m%dT%H%M%SZ', time.gmtime())}-{os.getpid()}"


def evaluate_gates(metrics: Dict[str, Any], baseline: Dict[str, Any]) -> Dict[str, Any]:
    """Fail closed: absent measurements cannot promote a candidate."""
    required = ("security_violations", "freshness_violations", "bq_recall", "bq_alpha", "bq_latency_ms", "bq_rss_mb", "quality_loss", "compiler_token_delta", "compiler_density", "tool_search")
    missing = [key for key in required if key not in metrics]
    if missing:
        return {"status": "pending", "promotion": False, "fallback": "baseline", "missing": missing}
    checks = {
        "security": metrics["security_violations"] == 0,
        "freshness": metrics["freshness_violations"] == 0,
        "bq": metrics["bq_recall"] >= baseline.get("bq_recall", 0) and metrics["bq_alpha"] >= baseline.get("bq_alpha", 0) and metrics["bq_latency_ms"] <= baseline.get("bq_latency_ms", float("inf")) and metrics["bq_rss_mb"] <= baseline.get("bq_rss_mb", float("inf")),
        "quality_loss": metrics["quality_loss"] <= baseline.get("max_quality_loss", 0),
        "compiler": metrics["compiler_token_delta"] <= baseline.get("max_token_delta", 0) and metrics["compiler_density"] >= baseline.get("min_density", 0),
        "tool_search": bool(metrics["tool_search"].get("recall")) and bool(metrics["tool_search"].get("precision")),
    }
    return {"status": "pass" if all(checks.values()) else "fail", "checks": checks, "promotion": all(checks.values()), "fallback": "baseline" if not all(checks.values()) else None}
