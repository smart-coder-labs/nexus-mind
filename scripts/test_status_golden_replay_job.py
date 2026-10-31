import unittest

from status_golden_replay_job import cpu_snapshot, legacy_result, pod_readiness_from_json, progress_from_logs, summary_from_logs


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

    def test_running_progress_aggregates_only_outcome_counts(self):
        logs = '\n'.join([
            '{"id":"secret-id","repository":"private/repo","merge_sha":"deadbeef","passed":true,"skipped":false,"error":null,"failure_output":"do not expose"}',
            '{"id":"secret-id-2","repository":"private/repo","merge_sha":"deadbeef","passed":false,"skipped":true,"error":null}',
            '{"id":"secret-id-3","repository":"private/repo","merge_sha":"deadbeef","passed":false,"skipped":false,"error":"cluster token leaked"}',
            '{"id":"secret-id-4","repository":"private/repo","merge_sha":"deadbeef","passed":false,"skipped":false,"error":"pod_not_ready"}',
            '{"summary":{"tasks":4,"passed":1,"failed":0,"skipped_no_evidence":1,"errors":2}}',
        ])
        progress = progress_from_logs(logs)
        self.assertEqual(progress, {"completed": 4, "passed": 1, "failed": 0, "skipped_no_evidence": 1, "errors": 2, "error_codes": {"other": 1, "pod_not_ready": 1}})
        self.assertNotIn("secret-id", str(progress))
        self.assertNotIn("deadbeef", str(progress))
        self.assertNotIn("do not expose", str(progress))
        self.assertNotIn("cluster token leaked", str(progress))

    def test_pod_readiness_aggregates_only_safe_phases_and_reasons(self):
        raw = '{"items":['
        raw += '{"metadata":{"name":"private-pod","labels":{"app":"factory-task"}},"status":{"phase":"Pending","containerStatuses":[{"state":{"waiting":{"reason":"ImagePullBackOff","message":"private registry credential"}}}]}}'
        raw += ',{"metadata":{"name":"unrelated-pod","labels":{"app":"other"}},"status":{"phase":"Running"}}'
        raw += ',{"metadata":{"name":"another-private-pod","labels":{"app":"factory-task"}},"status":{"phase":"Pending","conditions":[{"type":"PodScheduled","status":"False","reason":"Unschedulable","message":"0/1 nodes available: 1 Insufficient cpu; private node name"}]}}'
        raw += ']}'
        readiness = pod_readiness_from_json(raw)
        self.assertEqual(readiness, {
            "phases": {"Pending": 2},
            "waiting_reasons": {"ImagePullBackOff": 1, "Unschedulable": 1},
            "scheduling_causes": {"insufficient_cpu": 1},
            "workloads": {"other_factory": {"phases": {"Pending": 2}, "requested_cpu_m": 0}},
        })
        self.assertNotIn("private-pod", str(readiness))
        self.assertNotIn("private registry", str(readiness))
        self.assertNotIn("private node", str(readiness))

    def test_cpu_snapshot_counts_scheduled_requests_without_node_names(self):
        nodes = '{"items":[{"metadata":{"name":"private-node"},"status":{"allocatable":{"cpu":"2000m"}}}]}'
        pods = '{"items":[{"spec":{"nodeName":"private-node","containers":[{"resources":{"requests":{"cpu":"250m"}}},{"resources":{"requests":{"cpu":"250m"}}}],"initContainers":[{"resources":{"requests":{"cpu":"500m"}}}]},"status":{"phase":"Running"}},{"spec":{"nodeName":"private-node","containers":[{"resources":{"requests":{"cpu":"100m"}}}]},"status":{"phase":"Pending"}}]}'
        snapshot = cpu_snapshot(nodes, pods)
        self.assertEqual(snapshot, {"nodes": [{"allocatable_m": 2000, "requested_m": 600, "available_m": 1400}]})
        self.assertNotIn("private-node", str(snapshot))

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
