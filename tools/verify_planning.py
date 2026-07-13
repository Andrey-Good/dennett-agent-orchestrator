from __future__ import annotations

import json
import re
import sys
from pathlib import Path

from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parents[1]
errors: list[str] = []


def load_schema(name: str) -> Draft202012Validator:
    path = ROOT / "schemas" / name
    return Draft202012Validator(json.loads(path.read_text(encoding="utf-8")))


work_package_validator = load_schema("work-package.schema.json")
test_catalogue_validator = load_schema("test-catalogue.schema.json")

wp_id = re.compile(r"^WP-M\d{2}-\d{3}$")
test_id = re.compile(r"^TEST-[A-Z0-9-]+$")

work_packages: dict[str, str] = {}
for path in sorted((ROOT / "planning" / "milestones").glob("*.json")):
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except Exception as exc:
        errors.append(f"invalid milestone JSON {path.relative_to(ROOT)}: {exc}")
        continue
    for package in data.get("work_packages", []):
        for error in work_package_validator.iter_errors(package):
            errors.append(
                f"work package schema error in {path.relative_to(ROOT)} at "
                f"{'.'.join(str(part) for part in error.path) or '<root>'}: {error.message}"
            )
        ident = package.get("id", "")
        if not wp_id.fullmatch(ident):
            errors.append(f"invalid work package id {ident!r} in {path.relative_to(ROOT)}")
        if ident in work_packages:
            errors.append(f"duplicate work package id {ident} in {path.relative_to(ROOT)} and {work_packages[ident]}")
        work_packages[ident] = str(path.relative_to(ROOT))
        if package.get("status") == "READY" and not package.get("acceptance"):
            errors.append(f"READY package without acceptance tests: {ident}")

catalogue_ids: dict[str, str] = {}
for path in sorted((ROOT / "tests" / "catalog").glob("*.json")):
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except Exception as exc:
        errors.append(f"invalid test catalogue JSON {path.relative_to(ROOT)}: {exc}")
        continue
    for error in test_catalogue_validator.iter_errors(data):
        errors.append(
            f"test catalogue schema error in {path.relative_to(ROOT)} at "
            f"{'.'.join(str(part) for part in error.path) or '<root>'}: {error.message}"
        )
    for case in data.get("cases", []):
        ident = case.get("id", "")
        if not test_id.fullmatch(ident):
            errors.append(f"invalid test id {ident!r} in {path.relative_to(ROOT)}")
        if ident in catalogue_ids:
            errors.append(f"duplicate test id {ident} in {path.relative_to(ROOT)} and {catalogue_ids[ident]}")
        catalogue_ids[ident] = str(path.relative_to(ROOT))
        if case.get("priority") == "critical" and not case.get("requirement_refs"):
            errors.append(f"critical test without requirement refs: {ident}")

for path in sorted((ROOT / "planning" / "milestones").glob("*.json")):
    data = json.loads(path.read_text(encoding="utf-8"))
    for package in data.get("work_packages", []):
        for ident in package.get("acceptance", []):
            if ident not in catalogue_ids:
                errors.append(f"unknown test id {ident} referenced by {package.get('id')}")

if errors:
    print("Planning verification failed:")
    print("\n".join(f"- {error}" for error in errors))
    sys.exit(1)

print(
    f"Planning verification passed ({len(work_packages)} work packages, "
    f"{len(catalogue_ids)} test cases)."
)
