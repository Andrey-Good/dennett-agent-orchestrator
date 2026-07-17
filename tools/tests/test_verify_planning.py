from __future__ import annotations

from datetime import date
import json
from pathlib import Path
import shutil
from tempfile import TemporaryDirectory
from typing import Any, Callable
import unittest

from tools import verify_planning


ROOT = Path(__file__).resolve().parents[2]
TODAY = date(2026, 7, 17)


class PlanningValidatorTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary_directory = TemporaryDirectory()
        self.root = Path(self.temporary_directory.name)
        shutil.copytree(ROOT / "planning", self.root / "planning")
        shutil.copytree(ROOT / "tests" / "catalog", self.root / "tests" / "catalog")

    def tearDown(self) -> None:
        self.temporary_directory.cleanup()

    def read(self, relative: str) -> dict[str, Any]:
        return json.loads((self.root / relative).read_text(encoding="utf-8"))

    def write(self, relative: str, data: dict[str, Any]) -> None:
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(
            json.dumps(data, indent=2, ensure_ascii=False) + "\n",
            encoding="utf-8",
        )

    def update_package(
        self,
        package_id: str,
        update: Callable[[dict[str, Any]], None],
    ) -> None:
        relative = "planning/milestones/M00_repository_and_contracts.json"
        milestone = self.read(relative)
        package = next(
            item for item in milestone["work_packages"] if item["id"] == package_id
        )
        update(package)
        self.write(relative, milestone)

    def diagnostics(self) -> list[str]:
        return verify_planning.validate_planning(
            self.root,
            schema_root=ROOT / "schemas",
            today=TODAY,
        )

    def assert_diagnostic(self, diagnostics: list[str], *parts: str) -> None:
        if any(all(part in diagnostic for part in parts) for diagnostic in diagnostics):
            return
        self.fail(
            "No diagnostic contained all expected parts "
            f"{parts!r}:\n" + "\n".join(diagnostics)
        )

    def test_repository_planning_is_valid(self) -> None:
        self.assertEqual(
            verify_planning.validate_planning(ROOT, today=TODAY),
            [],
        )

    def test_duplicate_work_package_id_reports_both_locations(self) -> None:
        milestone = self.read(
            "planning/milestones/M00_repository_and_contracts.json"
        )
        package = next(
            item for item in milestone["work_packages"] if item["id"] == "WP-M00-004"
        )
        self.write("planning/work-packages/WP-M00-004.json", package)
        diagnostics = self.diagnostics()
        self.assert_diagnostic(
            diagnostics,
            "planning/work-packages/WP-M00-004.json:$.id",
            "duplicate work package id WP-M00-004",
            "planning/milestones/M00_repository_and_contracts.json",
        )

    def test_missing_references_and_dependency_cycles_are_rejected(self) -> None:
        def make_invalid(package: dict[str, Any]) -> None:
            package["depends_on"].extend(["WP-M00-005", "WP-M00-999"])
            package["acceptance"].append("TEST-MISSING-001")
            package["owner_decision"]["resolved_by"] = ["DEC-9999"]

        self.update_package("WP-M00-004", make_invalid)
        diagnostics = self.diagnostics()
        self.assert_diagnostic(
            diagnostics,
            "planning/milestones/M00_repository_and_contracts.json:$",
            "missing work package reference WP-M00-999",
        )
        self.assert_diagnostic(diagnostics, "dependency cycle", "WP-M00-004", "WP-M00-005")
        self.assert_diagnostic(
            diagnostics,
            "planning/milestones/M00_repository_and_contracts.json:$",
            "missing test catalogue reference TEST-MISSING-001",
        )
        self.assert_diagnostic(diagnostics, "missing decision reference DEC-9999")

    def test_ready_package_requires_evidence_closed_dependencies_and_decisions(self) -> None:
        def make_ready(package: dict[str, Any]) -> None:
            package["status"] = "READY"

        def remove_acceptance_evidence(package: dict[str, Any]) -> None:
            package["acceptance"] = []

        self.update_package(
            "WP-M00-004",
            lambda package: package.update(status="IN_PROGRESS"),
        )
        self.update_package("WP-M00-005", make_ready)
        self.update_package("WP-M00-007", remove_acceptance_evidence)
        decision = self.read("planning/decisions/DEC-0003.json")
        decision["work_package"] = "WP-M00-005"
        decision["severity"] = "red"
        decision["status"] = "open"
        decision["resolution"] = None
        self.write("planning/decisions/DEC-0003.json", decision)
        diagnostics = self.diagnostics()
        self.assert_diagnostic(diagnostics, "acceptance", "should be non-empty")
        self.assert_diagnostic(
            diagnostics,
            "READY package dependency WP-M00-004 is IN_PROGRESS, not MERGED",
        )
        self.assert_diagnostic(
            diagnostics,
            "status",
            "READY package has unresolved red decision DEC-0003",
        )

    def test_schema_errors_do_not_crash_graph_validation(self) -> None:
        batch = self.read("planning/batches/AB-M00-002.json")
        batch["execution"] = []
        self.write("planning/batches/AB-M00-002.json", batch)
        diagnostics = self.diagnostics()
        self.assert_diagnostic(
            diagnostics,
            "planning/batches/AB-M00-002.json:$.execution",
            "batch schema",
            "is not of type 'object'",
        )

    def test_owner_gate_blocks_start_and_resolution_must_match_package(self) -> None:
        def add_invalid_gate(package: dict[str, Any]) -> None:
            package["status"] = "IN_PROGRESS"
            package["owner_decision"]["required_before_start"] = True
            package["owner_decision"]["resolved_by"] = ["DEC-0001"]

        self.update_package("WP-M00-004", add_invalid_gate)
        diagnostics = self.diagnostics()
        self.assert_diagnostic(
            diagnostics,
            "owner_decision.required_before_start",
            "owner gate is unresolved for started package in IN_PROGRESS",
        )
        self.assert_diagnostic(
            diagnostics,
            "owner_decision.resolved_by[0]",
            "decision DEC-0001 belongs to WP-M00-006",
        )

    def test_unsafe_batch_risk_effects_and_parallel_writers_are_rejected(self) -> None:
        def make_risky(package: dict[str, Any]) -> None:
            package["risk"] = "R3"
            package["security_effects"]["external_effect"] = True
            package["allowed_roots"].append("../outside")

        self.update_package("WP-M00-004", make_risky)
        batch = self.read("planning/batches/AB-M00-002.json")
        batch["execution"]["max_parallel"] = 2
        batch["limits"]["max_risk"] = "R0"
        self.write("planning/batches/AB-M00-002.json", batch)
        diagnostics = self.diagnostics()
        self.assert_diagnostic(diagnostics, "R3 package requires an explicit owner gate")
        self.assert_diagnostic(diagnostics, "risk exceeds batch max_risk R0")
        self.assert_diagnostic(diagnostics, "WP-M00-004 declares an external effect")
        self.assert_diagnostic(
            diagnostics,
            "allowed_roots",
            "root must stay repository-relative: ../outside",
        )
        self.assert_diagnostic(
            diagnostics,
            "planning/batches/AB-M00-002.json:$.execution.max_parallel",
            "parallel packages",
            "overlap writer root",
        )

    def test_merged_package_requires_completion_evidence(self) -> None:
        (self.root / "planning" / "results" / "WP-M00-002.json").unlink()
        diagnostics = self.diagnostics()
        self.assert_diagnostic(
            diagnostics,
            "planning/milestones/M00_repository_and_contracts.json:$",
            "MERGED package lacks completion evidence",
        )

    def test_completed_result_records_every_acceptance_id(self) -> None:
        result = self.read("planning/results/WP-M00-002.json")
        result["work_package_result"]["requirements_satisfied"].remove(
            "TEST-PROTOCOL-BREAKING-001"
        )
        self.write("planning/results/WP-M00-002.json", result)
        diagnostics = self.diagnostics()
        self.assert_diagnostic(
            diagnostics,
            "planning/results/WP-M00-002.json:$.work_package_result.requirements_satisfied",
            "completed result lacks acceptance evidence TEST-PROTOCOL-BREAKING-001",
        )

    def test_blocked_result_may_have_empty_completion_arrays(self) -> None:
        self.update_package(
            "WP-M00-002",
            lambda package: package.update(status="BLOCKED"),
        )
        result = self.read("planning/results/WP-M00-002.json")
        packet = result["work_package_result"]
        packet["status"] = "blocked"
        packet["commits"] = []
        packet["changed_files"] = []
        packet["requirements_satisfied"] = []
        packet["tests"]["passed"] = []
        packet.pop("merged_at")
        self.write("planning/results/WP-M00-002.json", result)
        batch = self.read("planning/batches/AB-M00-002.json")
        batch["status"] = "DRAFT"
        self.write("planning/batches/AB-M00-002.json", batch)

        self.assertEqual(self.diagnostics(), [])

    def test_obsolete_debt_review_date_is_rejected(self) -> None:
        debt = self.read("planning/debt/DEBT-0001.json")
        debt["debt"]["deadline_or_review"] = "2026-07-16"
        self.write("planning/debt/DEBT-0001.json", debt)
        diagnostics = self.diagnostics()
        self.assert_diagnostic(
            diagnostics,
            "planning/debt/DEBT-0001.json:$.debt.deadline_or_review",
            "debt review date 2026-07-16 is obsolete",
        )


if __name__ == "__main__":
    unittest.main()
