#!/usr/bin/env python3
"""Harvest golden tasks for the software factory from merged pull requests.

One merged PR becomes one JSONL record: the ticket text a person wrote, the commit
an agent starts from (`base_sha`) and the accepted answer (`merge_sha`, never shown
to the agent). See openspec/changes/factory-f0-foundation/design.md §8.

The dataset is written OUTSIDE the repository (default ~/.nexusmind/evals/golden/v1):
client repositories are private and this repository is public, and an agent working
in a checkout must not be able to read the answers. Replaying a task clones the
repository fresh at base_sha with --depth 1; a full clone contains the answer.

Usage:
  python3 scripts/factory/harvest_golden_tasks.py \
      --repo smart-coder-labs/nexus-mind=30 --repo kasymir/kasymir-app-ui=20

Requires an authenticated `gh` CLI with read access to every repository.
"""

import argparse
import hashlib
import json
import os
import subprocess
import sys
import uuid

MAX_FILES = 40
BOT_LOGINS = ("dependabot", "renovate", "github-actions")
DEFAULT_REPOS = ["smart-coder-labs/nexus-mind=30", "kasymir/kasymir-app-ui=20"]
DEFAULT_OUT = os.path.expanduser("~/.nexusmind/evals/golden/v1")

# Same order as the Rust intake classifier: the most sensitive label wins.
LABEL_CLASSES = [
    ("security", {"security", "vulnerability", "auth"}),
    ("migration", {"migration", "database", "db"}),
    ("infra", {"infra", "infrastructure", "ci", "devops", "deploy"}),
    ("bugfix", {"bug", "bugfix", "defect"}),
    ("backend", {"backend", "api", "server"}),
    ("ui", {"ui", "frontend", "design"}),
    ("tests", {"tests", "test", "testing", "qa"}),
    ("docs", {"docs", "documentation", "doc"}),
]

UI_EXTENSIONS = (".tsx", ".jsx", ".css", ".scss", ".astro", ".vue", ".svelte")
UI_DIRS = ("apps/admin/", "apps/landing/", "apps/backoffice/", "src/components/", "src/pages/")


def stable_id(key):
    """Name-based UUID of `key`, identical to the Rust `stable_task_id`."""
    raw = bytearray(hashlib.sha256(key.encode()).digest()[:16])
    raw[6] = (raw[6] & 0x0F) | 0x50  # version 5 layout
    raw[8] = (raw[8] & 0x3F) | 0x80  # RFC 4122 variant
    return str(uuid.UUID(bytes=bytes(raw)))


def _is_doc(path):
    lower = path.lower()
    return lower.endswith(".md") or lower.startswith("docs/")


def _is_test(path):
    parts = path.split("/")
    name = parts[-1]
    return (
        any(part in ("tests", "test", "__tests__", "e2e") for part in parts[:-1])
        or ".test." in name
        or ".spec." in name
        or name.endswith(("_test.go", "_test.rs", "_test.py"))
        or (name.startswith("test_") and name.endswith(".py"))
    )


def _is_schema_migration(path):
    """A schema change: a `migrations/` directory, a `migrations.rs`, or SQL.
    A module merely named `migration` (e.g. knowledge migration) is not one."""
    parts = path.lower().split("/")
    return "migrations" in parts[:-1] or parts[-1] in ("migrations.rs", "migrations.py") or parts[-1].endswith(".sql")


def task_class(labels, paths):
    names = {(label.get("name") or "").strip().lower() for label in labels}
    for klass, aliases in LABEL_CLASSES:
        if names & aliases:
            return klass
    if any(_is_schema_migration(p) for p in paths):
        return "migration"
    if any(p.startswith(".github/") or "dockerfile" in p.lower() or p.endswith(("fly.toml", ".tf")) for p in paths):
        return "infra"
    if paths and all(_is_doc(p) for p in paths):
        return "docs"
    if paths and all(_is_test(p) or _is_doc(p) for p in paths):
        return "tests"
    if any(p.endswith(UI_EXTENSIONS) or p.startswith(UI_DIRS) for p in paths):
        return "ui"
    return "backend"


def skip_reason(pr):
    """Why a merged PR is not a usable golden task, or None when it is."""
    login = ((pr.get("author") or {}).get("login") or "").lower()
    if login.endswith("[bot]") or any(login.startswith(bot) for bot in BOT_LOGINS):
        return "bot_author"
    if not pr.get("baseRefOid") or not (pr.get("mergeCommit") or {}).get("oid"):
        return "missing_shas"
    if len(pr.get("files") or []) > MAX_FILES:
        return "too_many_files"
    if (pr.get("title") or "").lower().startswith("revert"):
        return "revert"
    if not (pr.get("body") or "").strip():
        return "no_ticket_text"
    return None


def to_golden_task(repository, pr, issues):
    """Normalizes a selected PR. `issues` maps issue number -> {title, body}."""
    closing = [ref.get("number") for ref in pr.get("closingIssuesReferences") or []]
    issue = next((issues[n] for n in closing if n in issues), None)
    if issue and (issue.get("body") or "").strip():
        task_text = f"{issue['title']}\n\n{issue['body'].strip()}"
    else:
        task_text = f"{pr['title']}\n\n{(pr.get('body') or '').strip()}"
    paths = [f["path"] for f in pr.get("files") or []]
    return {
        "id": stable_id(f"github_pr:{repository}#{pr['number']}"),
        "repository": repository,
        "pr_number": pr["number"],
        "title": pr["title"],
        "task_text": task_text,
        "base_sha": pr["baseRefOid"],
        "merge_sha": pr["mergeCommit"]["oid"],
        "changed_files": paths,
        "task_class": task_class(pr.get("labels") or [], paths),
        "acceptance": [],
    }


def _gh(args):
    completed = subprocess.run(["gh", *args], check=True, capture_output=True, text=True)
    return json.loads(completed.stdout)


def harvest(repository, count):
    fields = "number,title,body,author,baseRefOid,mergeCommit,files,labels,closingIssuesReferences"
    prs = _gh(["pr", "list", "-R", repository, "--state", "merged", "--limit", str(count * 4), "--json", fields])
    records, skipped = [], {}
    for pr in prs:
        if len(records) == count:
            break
        reason = skip_reason(pr)
        if reason:
            skipped[reason] = skipped.get(reason, 0) + 1
            continue
        issues = {}
        for ref in pr.get("closingIssuesReferences") or []:
            number = ref.get("number")
            try:
                issues[number] = _gh(["issue", "view", str(number), "-R", repository, "--json", "number,title,body"])
            except subprocess.CalledProcessError:
                pass  # issue in another repo or inaccessible: fall back to the PR body
        records.append(to_golden_task(repository, pr, issues))
    return records, skipped


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--repo", action="append", help="owner/name=count (repeatable)")
    parser.add_argument("--out", default=DEFAULT_OUT, help=f"output directory (default {DEFAULT_OUT})")
    args = parser.parse_args(argv)

    out = os.path.abspath(os.path.expanduser(args.out))
    repo_root = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
    if out == repo_root or out.startswith(repo_root + os.sep):
        sys.exit("refusing to write golden tasks inside the repository (answers and client data)")
    os.makedirs(out, exist_ok=True)

    for spec in args.repo or DEFAULT_REPOS:
        repository, _, count = spec.partition("=")
        records, skipped = harvest(repository, int(count or 25))
        path = os.path.join(out, repository.replace("/", "__") + ".jsonl")
        with open(path, "w", encoding="utf-8") as handle:
            for record in records:
                handle.write(json.dumps(record, ensure_ascii=False) + "\n")
        print(f"{repository}: {len(records)} tasks -> {path} (skipped: {skipped or 'none'})")


if __name__ == "__main__":
    main()
