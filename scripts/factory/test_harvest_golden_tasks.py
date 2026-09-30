"""Tests for the pure selection/normalization logic of harvest_golden_tasks.

Run: python3 -m unittest scripts/factory/test_harvest_golden_tasks.py
"""

import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(__file__))

import harvest_golden_tasks as h  # noqa: E402


def pr(**overrides):
    base = {
        "number": 42,
        "title": "Document MetricCard props",
        "body": "Adds JSDoc to every prop.",
        "author": {"login": "cesar"},
        "baseRefOid": "a" * 40,
        "mergeCommit": {"oid": "b" * 40},
        "files": [{"path": "docs/metric-card.md"}],
        "labels": [],
        "closingIssuesReferences": [],
    }
    base.update(overrides)
    return base


class SelectionTests(unittest.TestCase):
    def test_accepts_a_normal_pr(self):
        self.assertIsNone(h.skip_reason(pr()))

    def test_skips_bots_huge_prs_reverts_and_empty_tickets(self):
        self.assertEqual(h.skip_reason(pr(author={"login": "dependabot[bot]"})), "bot_author")
        self.assertEqual(h.skip_reason(pr(author={"login": "renovate"})), "bot_author")
        many = [{"path": f"src/f{i}.ts"} for i in range(41)]
        self.assertEqual(h.skip_reason(pr(files=many)), "too_many_files")
        self.assertEqual(h.skip_reason(pr(title='Revert "Add cache"')), "revert")
        self.assertEqual(h.skip_reason(pr(body="  ")), "no_ticket_text")

    def test_skips_prs_without_the_shas_needed_to_replay(self):
        self.assertEqual(h.skip_reason(pr(mergeCommit=None)), "missing_shas")
        self.assertEqual(h.skip_reason(pr(baseRefOid="")), "missing_shas")


class NormalizationTests(unittest.TestCase):
    def test_prefers_the_closing_issue_text(self):
        issue = {"number": 7, "title": "Props undocumented", "body": "MetricCard props lack docs."}
        record = h.to_golden_task("acme/web", pr(closingIssuesReferences=[{"number": 7}]), {7: issue})
        self.assertEqual(record["task_text"], "Props undocumented\n\nMetricCard props lack docs.")

    def test_record_shape_and_stable_id(self):
        record = h.to_golden_task("acme/web", pr(), {})
        self.assertEqual(record["id"], h.to_golden_task("acme/web", pr(), {})["id"])
        self.assertNotEqual(record["id"], h.to_golden_task("acme/web", pr(number=43), {})["id"])
        self.assertEqual(record["repository"], "acme/web")
        self.assertEqual(record["base_sha"], "a" * 40)
        self.assertEqual(record["merge_sha"], "b" * 40)
        self.assertEqual(record["changed_files"], ["docs/metric-card.md"])
        self.assertEqual(record["task_text"], "Document MetricCard props\n\nAdds JSDoc to every prop.")
        self.assertEqual(record["acceptance"], [])

    def test_task_class_from_labels_then_paths(self):
        self.assertEqual(h.task_class([{"name": "security"}, {"name": "docs"}], ["docs/a.md"]), "security")
        self.assertEqual(h.task_class([], ["docs/a.md", "README.md"]), "docs")
        self.assertEqual(h.task_class([], ["apps/backend/tests/x.rs", "src/a.test.ts"]), "tests")
        self.assertEqual(h.task_class([], ["apps/admin/src/pages/Home.tsx"]), "ui")
        self.assertEqual(h.task_class([], ["apps/backend/src/db/migrations.rs"]), "migration")
        self.assertEqual(h.task_class([], ["db/migrations/0042_add_index.sql"]), "migration")
        # A module merely NAMED "migration" (knowledge migration) is not a schema change.
        self.assertEqual(h.task_class([], ["apps/backend/src/migration/redact.rs"]), "backend")
        self.assertEqual(h.task_class([], ["apps/backend/src/api/x.rs"]), "backend")
        self.assertEqual(h.task_class([], [".github/workflows/ci.yml"]), "infra")


if __name__ == "__main__":
    unittest.main()
