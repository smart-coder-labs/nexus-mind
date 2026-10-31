import unittest

from status_golden_replay_job import legacy_result, summary_from_logs


class GoldenReplayStatusTests(unittest.TestCase):
    def test_exposes_only_numeric_summary_not_task_output(self):
        logs = '\n'.join(
            [
                '{"id":"task","failure_output":"private task output"}',
                '{"summary":{"tasks":50,"passed":40,"failed":2,"skipped_no_evidence":6,"errors":2}}',
            ]
        )
        self.assertEqual(
            summary_from_logs(logs),
            {"tasks": 50, "passed": 40, "failed": 2, "skipped_no_evidence": 6, "errors": 2},
        )

    def test_missing_summary_is_not_reported_as_complete(self):
        self.assertIsNone(summary_from_logs('{"id":"task","passed":true}'))

    def test_legacy_run_reports_complete_with_only_numeric_summary(self):
        status = legacy_result(
            "finished",
            '{"id":"task","failure_output":"private task output"}\n'
            '{"summary":{"tasks":50,"passed":40,"failed":2,"skipped_no_evidence":6,"errors":2}}',
        )
        self.assertEqual(
            status,
            {
                "state": "complete",
                "summary": {
                    "tasks": 50,
                    "passed": 40,
                    "failed": 2,
                    "skipped_no_evidence": 6,
                    "errors": 2,
                },
            },
        )
        self.assertNotIn("private task output", str(status))

    def test_legacy_running_process_is_not_treated_as_complete(self):
        self.assertEqual(legacy_result("running", '{"summary":{}}'), {"state": "running"})

    def test_legacy_finished_without_summary_is_incomplete(self):
        self.assertEqual(legacy_result("finished", ""), {"state": "incomplete"})


if __name__ == "__main__":
    unittest.main()
