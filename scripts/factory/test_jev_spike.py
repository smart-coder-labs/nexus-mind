"""Tests for the pure parts of the Jev spike (request building, parsing, stats).

Run: python3 -m unittest scripts/factory/test_jev_spike.py
"""

import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(__file__))

import jev_spike as j  # noqa: E402


class RequestTests(unittest.TestCase):
    def test_a_routing_decision_is_three_typed_questions(self):
        body = j.build_request({"title": "Fix typo in README", "description": "One word.", "paths": ["README.md"]})
        self.assertEqual(body["model"], "jev-latest")
        self.assertEqual(set(body["questions"]), {"task_class", "risk", "needs_human"})
        self.assertEqual(body["questions"]["task_class"]["type"], "choice")
        self.assertEqual(set(body["questions"]["task_class"]["criteria"]), set(j.TASK_CLASSES))
        self.assertEqual(body["questions"]["risk"]["type"], "score")
        self.assertEqual(len(body["questions"]["risk"]["criteria"]), 4)
        self.assertEqual(body["questions"]["needs_human"]["type"], "noul")
        self.assertEqual(body["state"]["paths"], ["README.md"])


class ParseTests(unittest.TestCase):
    RESPONSE = {
        "model": "jev-1.13.0",
        "answers": {
            "task_class": {"type": "choice", "choice": "docs", "probabilities": {"docs": 0.93}, "confidence": 0.93},
            "risk": {"type": "score", "score": 0.1, "legend": {}, "probabilities": {}, "confidence": 0.8},
            "needs_human": {"type": "noul", "noul": 0.12},
        },
        "usage": {"input_tokens": 2000, "output_tokens": 40},
    }

    def test_parses_a_complete_answer(self):
        parsed = j.parse_response(self.RESPONSE)
        self.assertEqual(parsed["task_class"], "docs")
        self.assertAlmostEqual(parsed["class_confidence"], 0.93)
        self.assertAlmostEqual(parsed["risk"], 0.1)
        self.assertAlmostEqual(parsed["needs_human"], 0.12)
        self.assertEqual(parsed["input_tokens"], 2000)
        self.assertEqual(parsed["model"], "jev-1.13.0")

    def test_a_missing_or_mistyped_answer_is_a_schema_violation(self):
        broken = {**self.RESPONSE, "answers": {**self.RESPONSE["answers"], "risk": {"type": "noul", "noul": 1}}}
        with self.assertRaises(j.SchemaViolation):
            j.parse_response(broken)
        with self.assertRaises(j.SchemaViolation):
            j.parse_response({"answers": {}})

    def test_cost_uses_the_input_price_only(self):
        self.assertAlmostEqual(j.cost_usd(2000), 0.000084)


class StatsTests(unittest.TestCase):
    def test_percentiles_and_accuracy(self):
        results = [
            {"expected": "docs", "task_class": "docs", "latency_ms": 100},
            {"expected": "security", "task_class": "security", "latency_ms": 200},
            {"expected": "tests", "task_class": "docs", "latency_ms": 300},
            {"expected": "ui", "error": "timeout", "latency_ms": 900},
        ]
        stats = j.summarize(results)
        self.assertEqual(stats["calls"], 4)
        self.assertEqual(stats["errors"], 1)
        self.assertAlmostEqual(stats["class_accuracy"], 2 / 3)
        self.assertEqual(stats["latency_p50_ms"], 200)
        self.assertEqual(stats["latency_p95_ms"], 900)

    def test_no_api_key_is_a_clear_error_and_never_echoes_anything(self):
        with self.assertRaises(SystemExit) as raised:
            j.api_key({})
        self.assertIn("JEV_API_KEY", str(raised.exception))


if __name__ == "__main__":
    unittest.main()
