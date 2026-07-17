from __future__ import annotations

import json
from pathlib import Path
from tempfile import TemporaryDirectory
from typing import Any
import unittest

from tools import generate_test_catalogue


ROOT = Path(__file__).resolve().parents[2]


class TestCatalogueGeneratorTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temporary_directory = TemporaryDirectory()
        self.root = Path(self.temporary_directory.name)
        (self.root / "tests" / "catalog").mkdir(parents=True)
        (self.root / "planning" / "milestones").mkdir(parents=True)
        (self.root / "tools").mkdir()
        (self.root / "tools" / "example_test.py").write_text(
            "def test_example():\n    return True\n",
            encoding="utf-8",
        )
        self.write_json(
            "tests/catalog/foundations.json",
            {"version": 1, "cases": [self.automated_case(), self.specified_case()]},
        )
        self.write_json(
            "planning/milestones/M00.json",
            {
                "id": "M00",
                "title": "Fixture milestone",
                "status": "ACTIVE",
                "work_packages": [
                    {
                        "id": "WP-M00-001",
                        "status": "IN_PROGRESS",
                        "acceptance": ["TEST-A-001"],
                    },
                    {
                        "id": "WP-M00-002",
                        "status": "READY",
                        "acceptance": ["TEST-B-001"],
                    },
                ],
            },
        )

    def tearDown(self) -> None:
        self.temporary_directory.cleanup()

    def write_json(self, relative: str, value: dict[str, Any]) -> None:
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(
            json.dumps(value, ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )

    def automated_case(self) -> dict[str, Any]:
        return {
            "id": "TEST-A-001",
            "title": "Automated fixture case",
            "status": "automated",
            "priority": "critical",
            "risk": "R2",
            "domains": ["planning", "repository"],
            "levels": ["static"],
            "requirement_refs": ["REQ-A"],
            "work_package_refs": ["WP-M00-001"],
            "expected": ["Observable result A"],
            "implementation": {
                "target_suite": "tools",
                "status": "automated",
                "test_paths": ["tools/example_test.py"],
            },
            "owner": "quality",
        }

    def specified_case(self) -> dict[str, Any]:
        return {
            "id": "TEST-B-001",
            "title": "Specified fixture case",
            "status": "specified",
            "priority": "critical",
            "risk": "R1",
            "domains": ["client"],
            "levels": ["integration"],
            "requirement_refs": ["REQ-B"],
            "work_package_refs": ["WP-M00-002"],
            "expected": ["Observable result B"],
            "implementation": {
                "target_suite": "tests/integration",
                "status": "planned",
                "test_paths": [],
            },
            "owner": "client",
        }

    def synchronize(self, *, check: bool = False) -> list[str]:
        return generate_test_catalogue.synchronize(
            self.root,
            check=check,
            schema_root=ROOT / "schemas",
        )

    def test_generates_all_views_deterministically(self) -> None:
        self.assertEqual(self.synchronize(), [])
        output_dir = self.root / "docs" / "testing" / "generated"
        first = {path.name: path.read_bytes() for path in sorted(output_dir.glob("*.md"))}
        self.assertEqual(set(first), set(generate_test_catalogue.OUTPUT_NAMES))
        self.assertEqual(self.synchronize(), [])
        second = {path.name: path.read_bytes() for path in sorted(output_dir.glob("*.md"))}
        self.assertEqual(first, second)
        for content in first.values():
            self.assertIn(generate_test_catalogue.NOTICE.encode(), content)
        self.assertIn(b"missing evidence", first["RELEASE_GATES.md"])
        self.assertIn(b"missing automation", first["TEST_DEBT.md"])
        self.assertIn(b"WP-M00-001", first["MILESTONE_TEST_PLAN.md"])

        context = generate_test_catalogue.load_context(
            self.root,
            schema_root=ROOT / "schemas",
        )
        reversed_context = generate_test_catalogue.CatalogueContext(
            root=context.root,
            cases=tuple(reversed(context.cases)),
            active_milestone=context.active_milestone,
            packages=context.packages,
            source_count=context.source_count,
        )
        self.assertEqual(
            generate_test_catalogue.render_views(context),
            generate_test_catalogue.render_views(reversed_context),
        )

    def test_check_reports_missing_stale_and_unexpected_views(self) -> None:
        self.synchronize()
        output_dir = self.root / "docs" / "testing" / "generated"
        (output_dir / "COVERAGE_MATRIX.md").unlink()
        (output_dir / "TEST_CATALOGUE.md").write_text("stale\n", encoding="utf-8")
        (output_dir / "OLD_VIEW.md").write_text("old\n", encoding="utf-8")

        diagnostics = self.synchronize(check=True)

        self.assertIn(
            "missing generated test catalogue view: "
            "docs/testing/generated/COVERAGE_MATRIX.md",
            diagnostics,
        )
        self.assertIn(
            "stale generated test catalogue view: "
            "docs/testing/generated/TEST_CATALOGUE.md",
            diagnostics,
        )
        self.assertIn(
            "unexpected generated test catalogue view: docs/testing/generated/OLD_VIEW.md",
            diagnostics,
        )

    def test_duplicate_ids_report_both_source_locations(self) -> None:
        self.write_json(
            "tests/catalog/duplicate.json",
            {"version": 1, "cases": [self.automated_case()]},
        )

        with self.assertRaises(generate_test_catalogue.CatalogueError) as raised:
            self.synchronize()

        message = str(raised.exception)
        self.assertIn("duplicate test id TEST-A-001", message)
        self.assertIn("tests/catalog/duplicate.json", message)
        self.assertIn("tests/catalog/foundations.json", message)

    def test_missing_or_escaping_implementation_path_is_rejected(self) -> None:
        catalogue = json.loads(
            (self.root / "tests" / "catalog" / "foundations.json").read_text(
                encoding="utf-8"
            )
        )
        catalogue["cases"][0]["implementation"]["test_paths"] = ["../outside.py"]
        self.write_json("tests/catalog/foundations.json", catalogue)

        with self.assertRaises(generate_test_catalogue.CatalogueError) as raised:
            self.synchronize()

        self.assertIn("implementation path does not exist: ../outside.py", str(raised.exception))

    def test_repository_generated_views_are_current(self) -> None:
        self.assertEqual(generate_test_catalogue.synchronize(ROOT, check=True), [])


if __name__ == "__main__":
    unittest.main()
