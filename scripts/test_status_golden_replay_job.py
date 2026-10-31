import unittest

from status_golden_replay_job import summary_from_logs


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


if __name__ == "__main__":
    unittest.main()
