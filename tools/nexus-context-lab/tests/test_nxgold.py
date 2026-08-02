import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from nxgold import canonical_hash, evaluate_gates, load_json, preflight, run_dir, validate_dataset, write_json
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from generate_synthetic_corpus import generate


FIXTURE = Path(__file__).resolve().parents[1] / "fixtures" / "nx_gold_sample.json"


class NxGoldContractTests(unittest.TestCase):
    def test_synthetic_corpus_counts_and_deterministic_hash(self):
        with tempfile.TemporaryDirectory() as temp:
            first = Path(temp) / "first"
            second = Path(temp) / "second"
            manifest_a = generate(first, chunks=30, seed=7)
            manifest_b = generate(second, chunks=30, seed=7)
            self.assertEqual(manifest_a["corpus_sha256"], manifest_b["corpus_sha256"])
            self.assertEqual(manifest_a["tenants"], ["tenant-fable", "tenant-orbit", "tenant-pine"])
            payload = json.loads((first / "corpus.json").read_text())
            self.assertEqual(len(payload["records"]), 30)
            self.assertEqual({item["tenant"] for item in payload["records"]}, set(manifest_a["tenants"]))
            self.assertGreater(manifest_a["files"]["sqlite"]["bytes"], 0)

    def test_runner_quick_writes_pending_local_artifacts(self):
        runner = Path(__file__).resolve().parents[1] / "scripts" / "run_benchmark.py"
        with tempfile.TemporaryDirectory() as temp:
            output = subprocess.check_output([
                sys.executable, str(runner), "--synthetic-chunks", "40", "--quick", "--window", "2",
                "--run-root", temp,
            ], text=True).strip()
            run_path = Path(output)
            results = json.loads((run_path / "results.json").read_text())
            self.assertTrue(results["synthetic_corpus"])
            self.assertEqual(results["nx_gold_status"], "pending")
            self.assertFalse(results["promotion"])
            self.assertEqual({p.name for p in run_path.iterdir()}, {
                "config.json", "gates.json", "inputs.json", "manifest.json", "results.json", "synthetic-corpus",
            })

    def test_sample_rejected_and_reports_pending(self):
        errors = validate_dataset(load_json(FIXTURE))
        self.assertTrue(errors)
        self.assertTrue(any("300 scenarios" in error for error in errors))
        self.assertTrue(any("sample/pending" in error for error in errors))

    def test_expected_distributions_are_explicit(self):
        data = load_json(FIXTURE)
        data["scenarios"] = []
        errors = validate_dataset(data)
        self.assertTrue(any("plane distribution" in error for error in errors))
        self.assertTrue(any("language distribution" in error for error in errors))

    def test_hash_is_canonical_and_deterministic(self):
        self.assertEqual(canonical_hash({"b": 2, "a": 1}), canonical_hash({"a": 1, "b": 2}))
        self.assertNotEqual(canonical_hash({"a": 1}), canonical_hash({"a": 2}))

    def test_preflight_rejects_productive_and_network_settings(self):
        with tempfile.TemporaryDirectory() as temp:
            workspace = Path(temp) / "nexus-local-qa"
            workspace.mkdir()
            env = {"NEXUS_LAB_WORKSPACE": str(workspace), "LOOPBACK_URL": "https://example.invalid", "CORS_ALLOWLIST": "https://example.invalid", "FAKE_API_KEY": "real-secret", "FAKE_DATABASE_URL": "postgres://prod", "MODEL_PATH": "/tmp/model", "MODEL_SHA256": "abc", "MODEL_PREFETCHED": "true", "EGRESS_ENABLED": "true", "MIN_FREE_BYTES": "0", "MIN_RAM_BYTES": "0", "MIN_CPU_COUNT": "0"}
            errors = preflight(env, Path(temp) / "repo")
            self.assertTrue(any("forbidden" in error for error in errors))
            self.assertTrue(any("loopback" in error for error in errors))
            self.assertTrue(any("EGRESS" in error for error in errors))

    def test_gate_failure_falls_back_to_baseline(self):
        metrics = {"security_violations": 1, "freshness_violations": 0, "bq_recall": 1, "bq_alpha": 1, "bq_latency_ms": 1, "bq_rss_mb": 1, "quality_loss": 0, "compiler_token_delta": 0, "compiler_density": 1, "tool_search": {"recall": 1, "precision": 1}}
        result = evaluate_gates(metrics, {"bq_recall": 0.9, "bq_alpha": 0.9, "bq_latency_ms": 10, "bq_rss_mb": 10, "max_quality_loss": 0.1, "max_token_delta": 1, "min_density": 0.1})
        self.assertEqual(result["status"], "fail")
        self.assertFalse(result["promotion"])
        self.assertEqual(result["fallback"], "baseline")

    def test_gate_missing_data_is_pending(self):
        result = evaluate_gates({}, {})
        self.assertEqual(result["status"], "pending")
        self.assertFalse(result["promotion"])

    def test_run_artifact_completeness_shape(self):
        with tempfile.TemporaryDirectory() as temp:
            out = run_dir(Path(temp))
            write_json(out / "manifest.json", {"schema": "NX-Gold v0"})
            write_json(out / "results.json", {"schema": "NX-Gold v0"})
            write_json(out / "gates.json", {"promotion": False})
            self.assertEqual({p.name for p in out.iterdir()}, {"manifest.json", "results.json", "gates.json"})
            for path in out.iterdir():
                self.assertEqual(json.loads(path.read_text())["schema"] if path.name != "gates.json" else json.loads(path.read_text())["promotion"], "NX-Gold v0" if path.name != "gates.json" else False)


if __name__ == "__main__":
    unittest.main()
