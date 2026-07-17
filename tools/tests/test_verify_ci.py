from __future__ import annotations

import json
from pathlib import Path
import shutil
import subprocess
from tempfile import TemporaryDirectory
import unittest

from tools import verify_ci, verify_worktree_clean


ROOT = Path(__file__).resolve().parents[2]


class CiConfigurationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary_directory = TemporaryDirectory()
        self.root = Path(self.temporary_directory.name)
        for relative in (
            verify_ci.WORKFLOW,
            verify_ci.PROTECTION,
            verify_ci.JUSTFILE,
            verify_ci.BOOTSTRAP,
        ):
            destination = self.root / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / relative, destination)

    def tearDown(self) -> None:
        self.temporary_directory.cleanup()

    def replace(self, relative: Path, old: str, new: str) -> None:
        path = self.root / relative
        text = path.read_text(encoding="utf-8")
        self.assertIn(old, text)
        path.write_text(text.replace(old, new), encoding="utf-8")

    def diagnostics_contain(self, text: str) -> bool:
        return any(text in item for item in verify_ci.validate(self.root))

    def test_repository_fast_gate_contract_is_valid(self) -> None:
        self.assertEqual(verify_ci.validate(ROOT), [])

    def test_unpinned_action_is_rejected_structurally(self) -> None:
        self.replace(verify_ci.WORKFLOW, verify_ci.CHECKOUT, "actions/checkout@v4")
        self.assertTrue(self.diagnostics_contain("steps differ"))

    def test_missing_or_masked_complete_gate_command_is_rejected(self) -> None:
        self.replace(verify_ci.WORKFLOW, "run: just check", "run: just verify")
        self.assertTrue(self.diagnostics_contain("steps differ"))
        self.replace(verify_ci.WORKFLOW, "run: just verify", "run: just check; true")
        self.assertTrue(self.diagnostics_contain("steps differ"))

    def test_comments_cannot_satisfy_a_missing_gate_step(self) -> None:
        self.replace(
            verify_ci.WORKFLOW,
            "        run: just check",
            "        # run: just check\n        run: just verify",
        )
        self.assertTrue(self.diagnostics_contain("steps differ"))

    def test_required_check_name_and_job_count_cannot_drift(self) -> None:
        self.replace(verify_ci.WORKFLOW, "    name: Fast Gate", "    name: CI")
        self.assertTrue(self.diagnostics_contain("fast-gate name"))
        path = self.root / verify_ci.WORKFLOW
        path.write_text(
            path.read_text(encoding="utf-8")
            + "  optional-job:\n    runs-on: ubuntu-24.04\n",
            encoding="utf-8",
        )
        self.assertTrue(self.diagnostics_contain("exactly one fast-gate job"))

    def test_privileged_trigger_and_continue_on_error_are_rejected(self) -> None:
        self.replace(
            verify_ci.WORKFLOW,
            "  pull_request:\n",
            "  pull_request_target:\n",
        )
        self.replace(
            verify_ci.WORKFLOW,
            "        run: just bootstrap",
            "        run: just bootstrap\n        continue-on-error: true",
        )
        diagnostics = verify_ci.validate(self.root)
        self.assertTrue(any("triggers must be" in item for item in diagnostics))
        self.assertTrue(any("steps differ" in item for item in diagnostics))

    def test_concurrency_contract_is_required(self) -> None:
        self.replace(
            verify_ci.WORKFLOW,
            "  cancel-in-progress: true",
            "  cancel-in-progress: false",
        )
        self.assertTrue(self.diagnostics_contain("concurrency contract differs"))

    def test_branch_protection_context_and_admin_enforcement_cannot_drift(self) -> None:
        path = self.root / verify_ci.PROTECTION
        data = json.loads(path.read_text(encoding="utf-8"))
        data["required_status_checks"]["contexts"] = ["CI"]
        data["enforce_admins"] = False
        path.write_text(json.dumps(data), encoding="utf-8")
        self.assertTrue(self.diagnostics_contain("approved contract"))

    def test_fast_gate_recipe_composition_cannot_drift(self) -> None:
        self.replace(
            verify_ci.JUSTFILE,
            "check: verify rust python ts test-contracts",
            "check: verify python ts",
        )
        self.assertTrue(self.diagnostics_contain("recipe check differs"))

    def test_bootstrap_requires_frozen_and_locked_dependency_commands(self) -> None:
        self.replace(
            verify_ci.BOOTSTRAP,
            'run(["corepack", "pnpm", "install", "--frozen-lockfile"])',
            'run(["corepack", "pnpm", "install"])',
        )
        self.assertTrue(self.diagnostics_contain("frozen bootstrap commands missing"))

    def test_worktree_probe_reports_tracked_and_untracked_drift(self) -> None:
        def runner(*_args: object, **_kwargs: object) -> subprocess.CompletedProcess[str]:
            return subprocess.CompletedProcess(
                args=["git", "status"],
                returncode=0,
                stdout=" M generated.txt\n?? untracked.txt\n",
                stderr="",
            )

        self.assertEqual(
            verify_worktree_clean.changed_entries(self.root, runner),
            [" M generated.txt", "?? untracked.txt"],
        )


if __name__ == "__main__":
    unittest.main()
