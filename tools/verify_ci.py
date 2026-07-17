"""Validate the required GitHub Fast Gate and its branch-protection contract."""

from __future__ import annotations

import json
from pathlib import Path
import re
import sys
from typing import Any


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = Path(".github/workflows/ci.yml")
PROTECTION = Path(".github/branch-protection.main.json")
JUSTFILE = Path("Justfile")
FULL_SHA = re.compile(r"^[0-9a-f]{40}$")
ACTION_LINE = re.compile(r"^\s*-\s+uses:\s+([^@\s]+)@([^\s#]+)")
JOB_LINE = re.compile(r"^  ([A-Za-z0-9_-]+):\s*$")
EXPECTED_ACTIONS = {
    "actions/checkout": "34e114876b0b11c390a56381ad16ebd13914f8d5",
    "jdx/mise-action": "5228313ee0372e111a38da051671ca30fc5a96db",
}


def _read(root: Path, relative: Path, diagnostics: list[str]) -> str | None:
    path = root / relative
    if not path.is_file():
        diagnostics.append(f"{relative.as_posix()}: missing required CI file")
        return None
    return path.read_text(encoding="utf-8")


def _job_ids(workflow: str) -> list[str]:
    lines = workflow.splitlines()
    try:
        start = lines.index("jobs:") + 1
    except ValueError:
        return []
    jobs: list[str] = []
    for line in lines[start:]:
        if line and not line.startswith(" "):
            break
        match = JOB_LINE.match(line)
        if match:
            jobs.append(match.group(1))
    return jobs


def _validate_workflow(workflow: str, diagnostics: list[str]) -> None:
    required_snippets = {
        "    name: Fast Gate": "job must expose the stable Fast Gate check name",
        "  pull_request:": "pull_request trigger is required",
        "  push:": "push trigger is required",
        "    branches: [main]": "push trigger must include main",
        "  merge_group:": "merge_group trigger is required",
        "  contents: read": "workflow permissions must be read-only",
        "    runs-on: ubuntu-24.04": "runner image must be pinned to ubuntu-24.04",
        "    timeout-minutes: 10": "fast gate must have a ten-minute timeout",
        "          fetch-depth: 0": "checkout must fetch the PR base for compatibility checks",
        '          version: "2026.7.7"': "mise itself must be pinned",
        "        run: just bootstrap": "frozen bootstrap must run",
        "        run: git diff --exit-code": "bootstrap drift must fail the gate",
        "        run: just check": "the complete documented gate must run",
    }
    for snippet, message in required_snippets.items():
        if snippet not in workflow:
            diagnostics.append(f"{WORKFLOW.as_posix()}: {message}")
    if not workflow.startswith("name: Fast Gate\n"):
        diagnostics.append(
            f"{WORKFLOW.as_posix()}: workflow must expose the stable Fast Gate name"
        )

    forbidden = {
        "pull_request_target": "pull_request_target is forbidden for repository code",
        "continue-on-error": "required checks cannot continue on error",
        "|| true": "required checks cannot mask command failures",
    }
    for token, message in forbidden.items():
        if token in workflow:
            diagnostics.append(f"{WORKFLOW.as_posix()}: {message}")

    jobs = _job_ids(workflow)
    if jobs != ["fast-gate"]:
        diagnostics.append(
            f"{WORKFLOW.as_posix()}: expected one fast-gate job, found {jobs!r}"
        )

    observed_actions: dict[str, str] = {}
    action_count = 0
    for number, line in enumerate(workflow.splitlines(), start=1):
        match = ACTION_LINE.match(line)
        if not match:
            continue
        action, reference = match.groups()
        action_count += 1
        observed_actions[action] = reference
        if not FULL_SHA.fullmatch(reference):
            diagnostics.append(
                f"{WORKFLOW.as_posix()}:{number}: action {action} is not pinned to a full SHA"
            )
    if action_count != len(EXPECTED_ACTIONS) or observed_actions != EXPECTED_ACTIONS:
        diagnostics.append(
            f"{WORKFLOW.as_posix()}: action set or pins differ from the approved map"
        )


def _validate_protection(data: Any, diagnostics: list[str]) -> None:
    if not isinstance(data, dict):
        diagnostics.append(f"{PROTECTION.as_posix()}: root must be an object")
        return
    checks = data.get("required_status_checks")
    expected_checks = {"strict": True, "contexts": ["Fast Gate"]}
    if checks != expected_checks:
        diagnostics.append(
            f"{PROTECTION.as_posix()}: required status checks must be {expected_checks!r}"
        )
    expected_values = {
        "branch": "main",
        "enforce_admins": False,
        "required_pull_request_reviews": None,
        "restrictions": None,
        "required_linear_history": False,
        "allow_force_pushes": False,
        "allow_deletions": False,
        "block_creations": False,
        "required_conversation_resolution": False,
        "lock_branch": False,
        "allow_fork_syncing": False,
    }
    for key, expected in expected_values.items():
        if data.get(key) != expected:
            diagnostics.append(
                f"{PROTECTION.as_posix()}: {key} must be {expected!r}"
            )


def validate(root: Path = ROOT) -> list[str]:
    diagnostics: list[str] = []
    workflow = _read(root, WORKFLOW, diagnostics)
    protection_text = _read(root, PROTECTION, diagnostics)
    justfile = _read(root, JUSTFILE, diagnostics)

    if workflow is not None:
        _validate_workflow(workflow, diagnostics)
    if protection_text is not None:
        try:
            protection = json.loads(protection_text)
        except json.JSONDecodeError as error:
            diagnostics.append(f"{PROTECTION.as_posix()}: invalid JSON: {error}")
        else:
            _validate_protection(protection, diagnostics)
    if justfile is not None:
        if "check: verify rust python ts test-contracts" not in justfile:
            diagnostics.append(
                f"{JUSTFILE.as_posix()}: check must compose verify, rust, python, ts and test-contracts"
            )
        invocation = (
            "mise exec -- uv run --project . --frozen python tools/verify_ci.py"
        )
        if invocation not in justfile:
            diagnostics.append(
                f"{JUSTFILE.as_posix()}: verify must execute tools/verify_ci.py"
            )
    return diagnostics


def main() -> int:
    diagnostics = validate()
    if diagnostics:
        print("CI configuration verification failed:", file=sys.stderr)
        for diagnostic in diagnostics:
            print(f"- {diagnostic}", file=sys.stderr)
        return 1
    print("CI configuration verification passed (required context: Fast Gate).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
