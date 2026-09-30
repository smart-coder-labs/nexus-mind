"""Tests for the pure evaluation logic of sandbox_drill.

Run: python3 -m unittest scripts/factory/test_sandbox_drill.py
"""

import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(__file__))

import sandbox_drill as d  # noqa: E402

CONFINED_STATUS = """Name:\tpython3
Uid:\t10001\t10001\t10001\t10001
CapEff:\t0000000000000000
CapPrm:\t0000000000000000
NoNewPrivs:\t1
Seccomp:\t2
"""


class ProcessConfinement(unittest.TestCase):
    def test_a_confined_process_passes(self):
        self.assertEqual(d.confinement_problems(CONFINED_STATUS), [])

    def test_each_weakness_is_named(self):
        status = CONFINED_STATUS.replace("Uid:\t10001", "Uid:\t0").replace(
            "CapEff:\t0000000000000000", "CapEff:\t00000000a80425fb"
        ).replace("NoNewPrivs:\t1", "NoNewPrivs:\t0").replace("Seccomp:\t2", "Seccomp:\t0")
        self.assertEqual(
            d.confinement_problems(status),
            ["runs_as_root", "has_capabilities", "privilege_escalation_allowed", "no_seccomp"],
        )

    def test_an_unreadable_status_fails_closed(self):
        self.assertEqual(d.confinement_problems(""), ["status_unreadable"])


class UserNamespace(unittest.TestCase):
    def test_a_mapped_range_passes(self):
        self.assertTrue(d.uid_map_is_isolated("         0 4110352384      65536\n"))

    def test_the_host_identity_map_fails(self):
        self.assertFalse(d.uid_map_is_isolated("         0          0 4294967295\n"))
        self.assertFalse(d.uid_map_is_isolated(""))


class Environment(unittest.TestCase):
    def test_placeholders_and_run_tokens_are_allowed(self):
        env = {
            "CLAUDE_CODE_OAUTH_TOKEN": "sandbox-placeholder-not-a-credential",
            "ANTHROPIC_BASE_URL": "http://proxy/r/v2.org.run.1.sig/anthropic",
            "HOME": "/home/task",
            "HTTPS_PROXY": "http://run:v2.org.run.1.sig@factory-egress-proxy:8080",
        }
        self.assertEqual(d.leaked_credentials(env), [])

    def test_real_credentials_are_reported_by_name_only(self):
        env = {
            "GITHUB_TOKEN": "ghs_" + "a" * 36,
            "NOTE": "key sk-ant-oat01-" + "b" * 40,
            "NEXUSMIND_API_KEY": "nm_live_" + "c" * 32,
            "DATABASE_URL": "postgres://u:p@db/x",
        }
        self.assertEqual(
            d.leaked_credentials(env),
            ["DATABASE_URL", "GITHUB_TOKEN", "NEXUSMIND_API_KEY", "NOTE"],
        )


class ProcessEnvironments(unittest.TestCase):
    AGENT = b"HOME=/home/task\0ANTHROPIC_BASE_URL=http://proxy/r/v2.org.run.1.sig/anthropic\0"
    COMMANDS = b"HOME=/tmp\0HTTPS_PROXY=http://run:v2.org.run-registry.1.sig@proxy:8080\0"

    def test_a_commands_pod_must_not_see_an_agent_token_anywhere(self):
        self.assertEqual(d.upstream_tokens_in([self.COMMANDS, self.COMMANDS]), 0)
        self.assertEqual(d.upstream_tokens_in([self.COMMANDS, self.AGENT]), 1)

    def test_the_proxy_is_found_from_either_variable(self):
        self.assertEqual(
            d.proxy_parts({"HTTPS_PROXY": "http://run:v2.o.r-registry.1.s@factory-egress-proxy:8080"}),
            ("factory-egress-proxy", 8080, "v2.o.r-registry.1.s"),
        )


class Network(unittest.TestCase):
    def test_an_unresolvable_name_is_inconclusive_not_blocked(self):
        blocked, detail = d.tcp("drill-name-that-does-not-exist.invalid", 443)
        self.assertFalse(blocked)
        self.assertIn("inconclusive", detail)


class Verdicts(unittest.TestCase):
    def test_the_drill_fails_if_any_attack_succeeds(self):
        results = [d.Result("net:internet", True, "blocked"), d.Result("fs:db", False, "readable")]
        self.assertEqual(d.exit_code(results), 1)
        self.assertEqual(d.exit_code(results[:1]), 0)

    def test_an_empty_drill_is_not_a_pass(self):
        self.assertEqual(d.exit_code([]), 1)


if __name__ == "__main__":
    unittest.main()
