import unittest

from create_golden_replay_job import build_job, validate_inputs


class GoldenReplayJobTests(unittest.TestCase):
    def setUp(self):
        self.deployment = {
            "spec": {
                "template": {
                    "spec": {
                        "serviceAccountName": "factory-worker",
                        "restartPolicy": "Always",
                        "volumes": [{"name": "data", "persistentVolumeClaim": {"claimName": "data"}}],
                        "initContainers": [{"name": "prepare", "image": "worker"}],
                        "containers": [
                            {"name": "backend", "image": "backend", "env": [{"name": "BACKEND_ONLY"}]},
                            {
                                "name": "autonomous-worker",
                                "image": "worker",
                                "command": ["/app/autonomous_worker"],
                                "env": [{"name": "DB_PATH", "value": "/data/db"}],
                                "volumeMounts": [{"name": "data", "mountPath": "/data"}],
                                "readinessProbe": {"exec": {"command": ["check"]}},
                            },
                        ],
                    }
                }
            }
        }

    def test_job_reuses_only_worker_runtime_and_keeps_persistent_runtime_context(self):
        job = build_job(
            self.deployment,
            "123456789",
            "bdf4b2aa-3f52-422e-8bee-a1babbb7d337",
            "f1-replay-123456789",
        )
        pod = job["spec"]["template"]["spec"]
        self.assertEqual(pod["serviceAccountName"], "factory-worker")
        self.assertEqual(pod["restartPolicy"], "Never")
        self.assertEqual([container["name"] for container in pod["containers"]], ["autonomous-worker"])
        worker = pod["containers"][0]
        self.assertEqual(worker["env"], [{"name": "DB_PATH", "value": "/data/db"}])
        self.assertNotIn("readinessProbe", worker)
        self.assertEqual(worker["command"], ["/bin/sh", "-ec"])
        self.assertEqual(worker["args"], ["exec /app/factory-golden-replay bdf4b2aa-3f52-422e-8bee-a1babbb7d337 < /factory-input/tasks.jsonl"])
        self.assertEqual(job["spec"]["ttlSecondsAfterFinished"], 604800)
        self.assertEqual(len(self.deployment["spec"]["template"]["spec"]["containers"]), 2)
        self.assertEqual(self.deployment["spec"]["template"]["spec"]["containers"][1]["command"], ["/app/autonomous_worker"])

    def test_identifiers_are_restricted_before_job_construction(self):
        with self.assertRaisesRegex(ValueError, "invalid_replay_id"):
            validate_inputs("../unsafe", "bdf4b2aa-3f52-422e-8bee-a1babbb7d337", "f1-replay-../unsafe")
        with self.assertRaisesRegex(ValueError, "invalid_config_map"):
            validate_inputs("123456789", "bdf4b2aa-3f52-422e-8bee-a1babbb7d337", "another-job")


if __name__ == "__main__":
    unittest.main()
