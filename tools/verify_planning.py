from __future__ import annotations

from dataclasses import dataclass
from datetime import date
import json
from pathlib import Path, PurePosixPath
import re
import sys
from typing import Any, Iterable

from jsonschema import Draft202012Validator


ROOT = Path(__file__).resolve().parents[1]
RISK_ORDER = {f"R{level}": level for level in range(5)}
ACTIVE_PACKAGE_STATES = {"IN_PROGRESS", "VERIFYING", "REVIEW", "MERGE_READY"}
EXECUTABLE_PACKAGE_STATES = {"READY", "MERGED"}
TERMINAL_DEPENDENCY_STATE = "MERGED"


@dataclass(frozen=True)
class Record:
    data: dict[str, Any]
    path: Path
    field: str = "$"


def json_field(base: str, parts: Iterable[object]) -> str:
    result = base
    for part in parts:
        if isinstance(part, int):
            result += f"[{part}]"
        else:
            result += f".{part}"
    return result


def roots_overlap(left: str, right: str) -> bool:
    left_path = PurePosixPath(left.replace("\\", "/"))
    right_path = PurePosixPath(right.replace("\\", "/"))
    return (
        left_path == right_path
        or left_path in right_path.parents
        or right_path in left_path.parents
    )


def root_is_repository_relative(root: str) -> bool:
    normalized = root.replace("\\", "/")
    path = PurePosixPath(normalized)
    return (
        bool(path.parts)
        and not path.is_absolute()
        and ".." not in path.parts
        and re.match(r"^[A-Za-z]:($|/)", normalized) is None
    )


class PlanningValidator:
    def __init__(
        self,
        root: Path = ROOT,
        *,
        schema_root: Path | None = None,
        today: date | None = None,
    ) -> None:
        self.root = root.resolve()
        self.schema_root = (schema_root or self.root / "schemas").resolve()
        self.today = today or date.today()
        self.diagnostics: list[str] = []
        self.validators = {
            name: self._load_validator(filename)
            for name, filename in {
                "milestone": "milestone.schema.json",
                "work_package": "work-package.schema.json",
                "batch": "autonomous-batch.schema.json",
                "decision": "decision-request.schema.json",
                "debt": "technical-debt.schema.json",
                "result": "work-package-result.schema.json",
                "test_catalogue": "test-catalogue.schema.json",
            }.items()
        }
        self.milestones: dict[str, Record] = {}
        self.packages: dict[str, Record] = {}
        self.batches: dict[str, Record] = {}
        self.decisions: dict[str, Record] = {}
        self.debts: dict[str, Record] = {}
        self.results: dict[str, Record] = {}
        self.tests: dict[str, Record] = {}
        self.vertical_slices: dict[str, Record] = {}

    def _load_validator(self, filename: str) -> Draft202012Validator:
        path = self.schema_root / filename
        schema = json.loads(path.read_text(encoding="utf-8"))
        Draft202012Validator.check_schema(schema)
        return Draft202012Validator(schema)

    def relative(self, path: Path) -> str:
        try:
            return path.resolve().relative_to(self.root).as_posix()
        except ValueError:
            return path.as_posix()

    def error(self, record: Record, field: str, message: str) -> None:
        location = field if field.startswith("$") else json_field(record.field, [field])
        self.diagnostics.append(f"{self.relative(record.path)}:{location}: {message}")

    def _read_json(self, path: Path) -> dict[str, Any] | None:
        try:
            value = json.loads(path.read_text(encoding="utf-8"))
        except json.JSONDecodeError as error:
            record = Record({}, path)
            self.error(
                record,
                "$",
                f"invalid JSON at line {error.lineno}, column {error.colno}: {error.msg}",
            )
            return None
        if not isinstance(value, dict):
            self.error(Record({}, path), "$", "top-level JSON value must be an object")
            return None
        return value

    def _schema(self, record: Record, validator_name: str) -> bool:
        validator = self.validators[validator_name]
        errors = sorted(
            validator.iter_errors(record.data),
            key=lambda error: tuple(str(part) for part in error.absolute_path),
        )
        for error in errors:
            field = json_field(record.field, error.absolute_path)
            self.error(record, field, f"{validator_name} schema: {error.message}")
        return not errors

    def _register(
        self,
        collection: dict[str, Record],
        record: Record,
        kind: str,
    ) -> None:
        identifier = record.data.get("id")
        if not isinstance(identifier, str):
            return
        previous = collection.get(identifier)
        if previous is not None:
            first = f"{self.relative(previous.path)}:{json_field(previous.field, ['id'])}"
            self.error(record, "id", f"duplicate {kind} id {identifier}; first declared at {first}")
            return
        collection[identifier] = record

    def _filename_matches(self, record: Record, expected: str, kind: str) -> None:
        if record.path.stem != expected:
            self.error(record, "id", f"{kind} id {expected} must match filename {record.path.name}")

    def load(self) -> None:
        self._load_milestones()
        self._load_standalone_packages()
        self._load_simple_records("batches", "batch", self.batches)
        self._load_simple_records("decisions", "decision", self.decisions)
        self._load_debts()
        self._load_results()
        self._load_test_catalogue()

    def _load_milestones(self) -> None:
        for path in sorted((self.root / "planning" / "milestones").glob("*.json")):
            data = self._read_json(path)
            if data is None:
                continue
            milestone = Record(data, path)
            if not self._schema(milestone, "milestone"):
                continue
            self._register(self.milestones, milestone, "milestone")
            identifier = data.get("id")
            if isinstance(identifier, str) and not (
                path.stem == identifier or path.stem.startswith(f"{identifier}_")
            ):
                self.error(
                    milestone,
                    "id",
                    f"milestone id {identifier} must prefix filename {path.name}",
                )
            for index, package_data in enumerate(data.get("work_packages", [])):
                if not isinstance(package_data, dict):
                    continue
                package = Record(package_data, path, f"$.work_packages[{index}]")
                if self._schema(package, "work_package"):
                    self._register(self.packages, package, "work package")
            for index, slice_data in enumerate(data.get("vertical_slices", [])):
                if not isinstance(slice_data, dict):
                    continue
                slice_record = Record(slice_data, path, f"$.vertical_slices[{index}]")
                self._register(self.vertical_slices, slice_record, "vertical slice")

    def _load_standalone_packages(self) -> None:
        folder = self.root / "planning" / "work-packages"
        for path in sorted(folder.glob("*.json")):
            data = self._read_json(path)
            if data is None:
                continue
            package = Record(data, path)
            if not self._schema(package, "work_package"):
                continue
            self._register(self.packages, package, "work package")
            identifier = data.get("id")
            if isinstance(identifier, str):
                self._filename_matches(package, identifier, "work package")

    def _load_simple_records(
        self,
        folder_name: str,
        validator_name: str,
        collection: dict[str, Record],
    ) -> None:
        folder = self.root / "planning" / folder_name
        for path in sorted(folder.glob("*.json")):
            data = self._read_json(path)
            if data is None:
                continue
            record = Record(data, path)
            if not self._schema(record, validator_name):
                continue
            self._register(collection, record, validator_name)
            identifier = data.get("id")
            if isinstance(identifier, str):
                self._filename_matches(record, identifier, validator_name)

    def _load_debts(self) -> None:
        for path in sorted((self.root / "planning" / "debt").glob("*.json")):
            data = self._read_json(path)
            if data is None:
                continue
            wrapper = Record(data, path)
            if not self._schema(wrapper, "debt"):
                continue
            debt_data = data.get("debt")
            if not isinstance(debt_data, dict):
                continue
            debt = Record(debt_data, path, "$.debt")
            self._register(self.debts, debt, "technical debt")
            identifier = debt_data.get("id")
            if isinstance(identifier, str):
                self._filename_matches(debt, identifier, "technical debt")

    def _load_results(self) -> None:
        for path in sorted((self.root / "planning" / "results").glob("*.json")):
            data = self._read_json(path)
            if data is None:
                continue
            wrapper = Record(data, path)
            if not self._schema(wrapper, "result"):
                continue
            result_data = data.get("work_package_result")
            if not isinstance(result_data, dict):
                continue
            result = Record(result_data, path, "$.work_package_result")
            self._register(self.results, result, "work package result")
            identifier = result_data.get("id")
            if isinstance(identifier, str):
                self._filename_matches(result, identifier, "work package result")

    def _load_test_catalogue(self) -> None:
        for path in sorted((self.root / "tests" / "catalog").glob("*.json")):
            data = self._read_json(path)
            if data is None:
                continue
            catalogue = Record(data, path)
            if not self._schema(catalogue, "test_catalogue"):
                continue
            for index, case_data in enumerate(data.get("cases", [])):
                if not isinstance(case_data, dict):
                    continue
                case = Record(case_data, path, f"$.cases[{index}]")
                self._register(self.tests, case, "test")

    def validate_graph(self) -> None:
        self._validate_milestone_links()
        self._validate_package_links()
        self._validate_dependency_cycles()
        self._validate_batches()
        self._validate_decisions()
        self._validate_debts()
        self._validate_results()

    def _validate_milestone_links(self) -> None:
        for milestone_id, milestone in self.milestones.items():
            listed: dict[str, int] = {}
            for slice_index, slice_data in enumerate(milestone.data.get("vertical_slices", [])):
                if not isinstance(slice_data, dict):
                    continue
                slice_id = slice_data.get("id")
                for package_index, package_id in enumerate(slice_data.get("work_packages", [])):
                    field = f"vertical_slices[{slice_index}].work_packages[{package_index}]"
                    package = self.packages.get(package_id)
                    if package is None:
                        self.error(milestone, field, f"missing work package reference {package_id}")
                        continue
                    listed[package_id] = listed.get(package_id, 0) + 1
                    if package.data.get("milestone") != milestone_id:
                        self.error(
                            package,
                            "milestone",
                            f"package is listed under milestone {milestone_id}",
                        )
                    if package.data.get("vertical_slice") != slice_id:
                        self.error(
                            package,
                            "vertical_slice",
                            f"package is listed under slice {slice_id}",
                        )
            for package_id, package in self.packages.items():
                if package.data.get("milestone") != milestone_id:
                    continue
                count = listed.get(package_id, 0)
                if count != 1:
                    self.error(
                        package,
                        "vertical_slice",
                        f"package must appear in exactly one milestone slice; found {count}",
                    )
            for index, decision_id in enumerate(milestone.data.get("owner_decisions", [])):
                if decision_id not in self.decisions:
                    self.error(
                        milestone,
                        f"owner_decisions[{index}]",
                        f"missing decision reference {decision_id}",
                    )
            for index, batch_id in enumerate(milestone.data.get("batches", [])):
                if batch_id not in self.batches:
                    self.error(
                        milestone,
                        f"batches[{index}]",
                        f"missing batch reference {batch_id}",
                    )

    def _validate_package_links(self) -> None:
        decisions_by_package: dict[str, list[Record]] = {}
        for decision in self.decisions.values():
            package_id = str(decision.data.get("work_package"))
            decisions_by_package.setdefault(package_id, []).append(decision)

        for package_id, package in self.packages.items():
            milestone_id = package.data.get("milestone")
            slice_id = package.data.get("vertical_slice")
            if milestone_id not in self.milestones:
                self.error(package, "milestone", f"missing milestone reference {milestone_id}")
            if slice_id not in self.vertical_slices:
                self.error(
                    package,
                    "vertical_slice",
                    f"missing vertical slice reference {slice_id}",
                )
            for index, dependency in enumerate(package.data.get("depends_on", [])):
                if dependency == package_id:
                    self.error(package, f"depends_on[{index}]", "package cannot depend on itself")
                elif dependency not in self.packages:
                    self.error(
                        package,
                        f"depends_on[{index}]",
                        f"missing work package reference {dependency}",
                    )
            for index, test_id in enumerate(package.data.get("acceptance", [])):
                test = self.tests.get(test_id)
                if test is None:
                    self.error(
                        package,
                        f"acceptance[{index}]",
                        f"missing test catalogue reference {test_id}",
                    )
                elif test.data.get("status") == "retired":
                    self.error(
                        package,
                        f"acceptance[{index}]",
                        f"acceptance test {test_id} is retired",
                    )
            self._validate_root_boundary(package)

            risk = package.data.get("risk")
            owner_gate = package.data.get("owner_decision", {})
            resolved_by = owner_gate.get("resolved_by", [])
            for index, decision_id in enumerate(resolved_by):
                decision = self.decisions.get(decision_id)
                if decision is None:
                    self.error(
                        package,
                        f"owner_decision.resolved_by[{index}]",
                        f"missing decision reference {decision_id}",
                    )
                elif decision.data.get("status") != "resolved":
                    self.error(
                        package,
                        f"owner_decision.resolved_by[{index}]",
                        f"decision {decision_id} is not resolved",
                    )
                elif decision.data.get("work_package") != package_id:
                    self.error(
                        package,
                        f"owner_decision.resolved_by[{index}]",
                        f"decision {decision_id} belongs to "
                        f"{decision.data.get('work_package')}",
                    )
            has_owner_gate = (
                owner_gate.get("required_before_start") is True
                or owner_gate.get("required_before_merge") is True
                or bool(resolved_by)
            )
            if risk in {"R3", "R4"} and not has_owner_gate:
                self.error(
                    package,
                    "owner_decision",
                    f"{risk} package requires an explicit owner gate",
                )

            status = package.data.get("status")
            started_states = {
                "IN_PROGRESS",
                "VERIFYING",
                "REVIEW",
                "MERGE_READY",
                "MERGED",
            }
            if owner_gate.get("required_before_start") is True and status in started_states:
                self.error(
                    package,
                    "owner_decision.required_before_start",
                    f"owner gate is unresolved for started package in {status}",
                )
            if owner_gate.get("required_before_merge") is True and status in {
                "MERGE_READY",
                "MERGED",
            }:
                self.error(
                    package,
                    "owner_decision.required_before_merge",
                    f"owner gate is unresolved for package in {status}",
                )

            if status == "READY":
                for index, dependency in enumerate(package.data.get("depends_on", [])):
                    dependency_record = self.packages.get(dependency)
                    if (
                        dependency_record
                        and dependency_record.data.get("status")
                        != TERMINAL_DEPENDENCY_STATE
                    ):
                        dependency_status = dependency_record.data.get("status")
                        self.error(
                            package,
                            f"depends_on[{index}]",
                            f"READY package dependency {dependency} is "
                            f"{dependency_status}, not MERGED",
                        )
                for decision in decisions_by_package.get(package_id, []):
                    is_open_red = (
                        decision.data.get("severity") == "red"
                        and decision.data.get("status") == "open"
                    )
                    if is_open_red:
                        self.error(
                            package,
                            "status",
                            "READY package has unresolved red decision "
                            f"{decision.data.get('id')}",
                        )

            if status in {"MERGE_READY", "MERGED"}:
                result = self.results.get(package_id)
                if result is None:
                    self.error(
                        package,
                        "status",
                        f"{status} package lacks completion evidence",
                    )
                elif result.data.get("status") != "completed":
                    self.error(
                        package,
                        "status",
                        f"completion evidence is {result.data.get('status')}, not completed",
                    )

    def _validate_root_boundary(self, package: Record) -> None:
        for field in ("allowed_roots", "forbidden_roots"):
            for index, root in enumerate(package.data.get(field, [])):
                if not root_is_repository_relative(str(root)):
                    self.error(
                        package,
                        f"{field}[{index}]",
                        f"root must stay repository-relative: {root}",
                    )
        for allowed_index, allowed in enumerate(package.data.get("allowed_roots", [])):
            for forbidden_index, forbidden in enumerate(package.data.get("forbidden_roots", [])):
                if roots_overlap(str(allowed), str(forbidden)):
                    self.error(
                        package,
                        f"allowed_roots[{allowed_index}]",
                        f"overlaps forbidden_roots[{forbidden_index}] ({forbidden})",
                    )

    def _validate_dependency_cycles(self) -> None:
        state: dict[str, int] = {}
        stack: list[str] = []
        emitted: set[frozenset[str]] = set()

        def visit(package_id: str) -> None:
            state[package_id] = 1
            stack.append(package_id)
            for dependency in self.packages[package_id].data.get("depends_on", []):
                if dependency not in self.packages:
                    continue
                if state.get(dependency, 0) == 0:
                    visit(dependency)
                elif state.get(dependency) == 1:
                    start = stack.index(dependency)
                    cycle = stack[start:] + [dependency]
                    key = frozenset(cycle)
                    if key not in emitted:
                        emitted.add(key)
                        self.error(
                            self.packages[package_id],
                            "depends_on",
                            f"dependency cycle: {' -> '.join(cycle)}",
                        )
            stack.pop()
            state[package_id] = 2

        for package_id in sorted(self.packages):
            if state.get(package_id, 0) == 0:
                visit(package_id)

    def _depends_transitively(self, package_id: str, target: str) -> bool:
        package = self.packages.get(package_id)
        if package is None:
            return False
        pending = list(package.data.get("depends_on", []))
        seen: set[str] = set()
        while pending:
            current = pending.pop()
            if current == target:
                return True
            if current in seen or current not in self.packages:
                continue
            seen.add(current)
            pending.extend(self.packages[current].data.get("depends_on", []))
        return False

    def _validate_batches(self) -> None:
        for batch_id, batch in self.batches.items():
            package_ids = list(batch.data.get("packages", []))
            package_records: list[Record] = []
            for index, package_id in enumerate(package_ids):
                package = self.packages.get(package_id)
                if package is None:
                    self.error(
                        batch,
                        f"packages[{index}]",
                        f"missing work package reference {package_id}",
                    )
                else:
                    package_records.append(package)
            execution = batch.data.get("execution", {})
            if execution.get("max_parallel", 0) > len(package_ids):
                self.error(batch, "execution.max_parallel", "cannot exceed package count")
            if execution.get("stop_after_packages", 0) > len(package_ids):
                self.error(batch, "execution.stop_after_packages", "cannot exceed package count")

            max_risk = batch.data.get("limits", {}).get("max_risk")
            for index, package_id in enumerate(package_ids):
                package = self.packages.get(package_id)
                package_risk = RISK_ORDER.get(package.data.get("risk"), 99) if package else 99
                if package and package_risk > RISK_ORDER.get(max_risk, -1):
                    self.error(
                        batch,
                        f"packages[{index}]",
                        f"{package_id} risk exceeds batch max_risk {max_risk}",
                    )
                if (
                    package
                    and batch.data.get("limits", {}).get("external_effects") == "forbidden"
                    and package.data.get("security_effects", {}).get("external_effect") is True
                ):
                    self.error(
                        batch,
                        f"packages[{index}]",
                        f"{package_id} declares an external effect",
                    )

            positions = {package_id: index for index, package_id in enumerate(package_ids)}
            if execution.get("order") == "dependency":
                for index, package_id in enumerate(package_ids):
                    package = self.packages.get(package_id)
                    if package is None:
                        continue
                    for dependency in package.data.get("depends_on", []):
                        if dependency in positions and positions[dependency] >= index:
                            self.error(
                                batch,
                                f"packages[{index}]",
                                f"{package_id} appears before dependency {dependency}",
                            )
                        elif (
                            dependency not in positions
                            and batch.data.get("status") in {"READY", "RUNNING"}
                            and dependency in self.packages
                            and self.packages[dependency].data.get("status") != "MERGED"
                        ):
                            self.error(
                                batch,
                                f"packages[{index}]",
                                f"external dependency {dependency} is not MERGED",
                            )

            if execution.get("max_parallel", 1) > 1:
                self._validate_parallel_writers(batch, package_ids)

            status = batch.data.get("status")
            package_statuses = [package.data.get("status") for package in package_records]
            if status == "READY":
                for index, package_status in enumerate(package_statuses):
                    if package_status not in EXECUTABLE_PACKAGE_STATES:
                        self.error(
                            batch,
                            f"packages[{index}]",
                            f"READY batch contains {package_status} package",
                        )
            elif status == "RUNNING" and not any(
                package_status in ACTIVE_PACKAGE_STATES for package_status in package_statuses
            ):
                self.error(batch, "status", "RUNNING batch has no active package")
            elif status == "COMPLETED" and any(
                package_status != "MERGED" for package_status in package_statuses
            ):
                self.error(batch, "status", "COMPLETED batch contains a package that is not MERGED")

    def _validate_parallel_writers(self, batch: Record, package_ids: list[str]) -> None:
        for left_index, left_id in enumerate(package_ids):
            left = self.packages.get(left_id)
            if left is None:
                continue
            for right_index in range(left_index + 1, len(package_ids)):
                right_id = package_ids[right_index]
                right = self.packages.get(right_id)
                if right is None:
                    continue
                packages_are_ordered = self._depends_transitively(
                    left_id, right_id
                ) or self._depends_transitively(right_id, left_id)
                if packages_are_ordered:
                    continue
                for left_root in left.data.get("allowed_roots", []):
                    for right_root in right.data.get("allowed_roots", []):
                        if roots_overlap(str(left_root), str(right_root)):
                            self.error(
                                batch,
                                "execution.max_parallel",
                                f"parallel packages {left_id} and {right_id} overlap writer "
                                f"root {left_root} / {right_root}",
                            )
                            break
                    else:
                        continue
                    break

    def _validate_decisions(self) -> None:
        for decision_id, decision in self.decisions.items():
            package_id = decision.data.get("work_package")
            if package_id not in self.packages:
                self.error(decision, "work_package", f"missing work package reference {package_id}")
            options = decision.data.get("options", [])
            option_ids = [option.get("id") for option in options if isinstance(option, dict)]
            duplicates = sorted(
                {option_id for option_id in option_ids if option_ids.count(option_id) > 1}
            )
            if duplicates:
                self.error(decision, "options", f"duplicate option ids: {', '.join(duplicates)}")
            recommendation = decision.data.get("recommendation")
            if recommendation is not None and recommendation not in option_ids:
                self.error(decision, "recommendation", f"unknown option {recommendation}")
            default = decision.data.get("default_if_deferred")
            if default not in {"stop", "safe_noop"} and default not in option_ids:
                self.error(decision, "default_if_deferred", f"unknown option {default}")
            resolution = decision.data.get("resolution")
            if decision.data.get("status") == "resolved":
                if not isinstance(resolution, dict):
                    self.error(
                        decision,
                        "resolution",
                        "resolved decision requires resolution evidence",
                    )
                elif resolution.get("selected_option") not in option_ids:
                    self.error(
                        decision,
                        "resolution.selected_option",
                        "selected option is not declared",
                    )
            elif resolution is not None:
                self.error(
                    decision,
                    "resolution",
                    "non-resolved decision cannot contain resolution evidence",
                )

    def _validate_debts(self) -> None:
        iso_date = re.compile(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}$")
        for debt in self.debts.values():
            for index, test_id in enumerate(debt.data.get("tests_protecting_behavior", [])):
                if test_id not in self.tests:
                    self.error(
                        debt,
                        f"tests_protecting_behavior[{index}]",
                        f"missing test reference {test_id}",
                    )
            deadline = debt.data.get("deadline_or_review")
            if isinstance(deadline, str) and iso_date.fullmatch(deadline):
                try:
                    review_date = date.fromisoformat(deadline)
                except ValueError:
                    self.error(debt, "deadline_or_review", f"invalid ISO review date {deadline}")
                else:
                    if review_date < self.today:
                        self.error(
                            debt,
                            "deadline_or_review",
                            f"debt review date {deadline} is obsolete",
                        )

    def _validate_results(self) -> None:
        for package_id, result in self.results.items():
            package = self.packages.get(package_id)
            if package is None:
                self.error(result, "id", f"missing work package reference {package_id}")
                continue
            if result.data.get("status") == "completed":
                requirements = set(result.data.get("requirements_satisfied", []))
                for test_id in package.data.get("acceptance", []):
                    if test_id not in requirements:
                        self.error(
                            result,
                            "requirements_satisfied",
                            f"completed result lacks acceptance evidence {test_id}",
                        )
                if result.data.get("tests", {}).get("failed"):
                    self.error(result, "tests.failed", "completed result contains failed tests")
                if result.data.get("review", {}).get("findings_open"):
                    self.error(
                        result,
                        "review.findings_open",
                        "completed result contains open findings",
                    )
                if result.data.get("owner_decision_required") is True:
                    self.error(
                        result,
                        "owner_decision_required",
                        "completed result still requires owner decision",
                    )
            if package.data.get("status") == "MERGED" and not result.data.get("merged_at"):
                self.error(result, "merged_at", "MERGED package result requires merged_at")

    def run(self) -> list[str]:
        self.load()
        self.validate_graph()
        return sorted(set(self.diagnostics))

    def success_summary(self) -> str:
        return (
            "Planning verification passed "
            f"({len(self.milestones)} milestones, {len(self.packages)} work packages, "
            f"{len(self.batches)} batches, {len(self.decisions)} decisions, "
            f"{len(self.debts)} debts, {len(self.results)} results, {len(self.tests)} tests)."
        )


def validate_planning(
    root: Path = ROOT,
    *,
    schema_root: Path | None = None,
    today: date | None = None,
) -> list[str]:
    return PlanningValidator(root, schema_root=schema_root, today=today).run()


def main() -> int:
    validator = PlanningValidator()
    diagnostics = validator.run()
    if diagnostics:
        print(f"Planning verification failed ({len(diagnostics)} diagnostics):")
        print("\n".join(f"- {diagnostic}" for diagnostic in diagnostics))
        return 1
    print(validator.success_summary())
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
