from __future__ import annotations

import json
from pathlib import Path
import shutil
from tempfile import TemporaryDirectory
import unittest

from tools import verify_ci


ROOT = Path(__file__).resolve().parents[2]


class CiConfigurationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary_directory = TemporaryDirectory()
        self.root = Path(self.temporary_directory.name)
        (self.root / ".github" / "workflows").mkdir(parents=True)
        shutil.copy2(ROOT / verify_ci.WORKFLOW, self.root / verify_ci.WORKFLOW)
        shutil.copy2(ROOT / verify_ci.PROTECTION, self.root / verify_ci.PROTECTION)
        shutil.copy2(ROOT / verify_ci.JUSTFILE, self.root / verify_ci.JUSTFILE)

    def tearDown(self) -> None:
        self.temporary_directory.cleanup()

    def replace_workflow(self, old: str, new: str) -> None:
        path = self.root / verify_ci.WORKFLOW
        text = path.read_text(encoding="utf-8")
        self.assertIn(old, text)
        path.write_text(text.replace(old, new), encoding="utf-8")

    def test_repository_fast_gate_contract_is_valid(self) -> None:
        self.assertEqual(verify_ci.validate(ROOT), [])

    def test_unpinned_action_is_rejected(self) -> None:
        self.replace_workflow(
            verify_ci.EXPECTED_ACTIONS["actions/checkout"],
            "v4",
        )
        diagnostics = verify_ci.validate(self.root)
        self.assertTrue(any("not pinned to a full SHA" in item for item in diagnostics))
        self.assertTrue(any("approved map" in item for item in diagnostics))

    def test_missing_complete_gate_command_is_rejected(self) -> None:
        self.replace_workflow("run: just check", "run: just verify")
        diagnostics = verify_ci.validate(self.root)
        self.assertTrue(any("complete documented gate" in item for item in diagnostics))

    def test_required_check_name_cannot_drift(self) -> None:
        self.replace_workflow("    name: Fast Gate", "    name: CI")
        diagnostics = verify_ci.validate(self.root)
        self.assertTrue(any("stable Fast Gate check name" in item for item in diagnostics))

    def test_failure_masking_and_privileged_pr_trigger_are_rejected(self) -> None:
        self.replace_workflow(
            "  pull_request:\n",
            "  pull_request_target:\n",
        )
        path = self.root / verify_ci.WORKFLOW
        path.write_text(
            path.read_text(encoding="utf-8") + "\ncontinue-on-error: true\n",
            encoding="utf-8",
        )
        diagnostics = verify_ci.validate(self.root)
        self.assertTrue(any("pull_request_target is forbidden" in item for item in diagnostics))
        self.assertTrue(any("cannot continue on error" in item for item in diagnostics))

    def test_extra_job_is_rejected(self) -> None:
        path = self.root / verify_ci.WORKFLOW
        path.write_text(
            path.read_text(encoding="utf-8") + "  optional-job:\n    runs-on: ubuntu-24.04\n",
            encoding="utf-8",
        )
        diagnostics = verify_ci.validate(self.root)
        self.assertTrue(any("expected one fast-gate job" in item for item in diagnostics))

    def test_branch_protection_context_cannot_drift(self) -> None:
        path = self.root / verify_ci.PROTECTION
        data = json.loads(path.read_text(encoding="utf-8"))
        data["required_status_checks"]["contexts"] = ["CI"]
        path.write_text(json.dumps(data), encoding="utf-8")
        diagnostics = verify_ci.validate(self.root)
        self.assertTrue(any("required status checks" in item for item in diagnostics))


if __name__ == "__main__":
    unittest.main()
