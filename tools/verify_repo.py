from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Iterable


ROOT = Path(__file__).resolve().parents[1]
REQUIRED = [
    "README.md",
    "AGENTS.md",
    "Cargo.toml",
    "docs/specifications/00_Dennett_Functional_Concept.md",
    "docs/architecture/80_Dennett_System_Architecture_and_Runtime_Topology.md",
    "docs/architecture/83_Dennett_Client_Operations_Testing_and_Implementation_Blueprint.md",
    "crates/dennett-contracts/src/lib.rs",
    "services/head/src/main.rs",
    "apps/desktop/AGENTS.md",
    "apps/mobile/AGENTS.md",
    "tests/scenarios/head-promotion-opt-in.yaml",
    "docs/implementation/00_IMPLEMENTATION_AND_EVOLUTION_STRATEGY.md",
    "docs/implementation/01_AGENT_EXECUTION_PROTOCOL.md",
    "docs/implementation/02_OWNER_PLAYBOOK.md",
    "docs/implementation/03_WORK_PACKAGE_SYSTEM.md",
    "docs/implementation/04_MILESTONE_DEPENDENCY_MAP.md",
    "docs/testing/TEST_CATALOGUE_AND_QUALITY_GATES.md",
    "planning/README.md",
    "tests/catalog/foundations.seed.json",
    "schemas/work-package.schema.json",
    "tools/generate_repository_metadata.py",
]
INSTRUCTION_ROOTS = [
    "apps/desktop",
    "apps/mobile",
    "services/head",
    "services/node",
    "services/memoryd",
    "services/sensor-worker",
    "crates/dennett-memory-core",
    "crates/dennett-trust-core",
    "adapters",
    "protocols",
    "tests/scenarios",
]
TEXT_SUFFIXES = {
    ".json",
    ".md",
    ".proto",
    ".py",
    ".rs",
    ".toml",
    ".ts",
    ".tsx",
    ".txt",
    ".yaml",
    ".yml",
}
SPECIAL_TEXT_FILES = {".env.example", ".gitattributes", ".gitignore", "Justfile"}
ALLOWED_LEGACY_TEXT = {"planning/decisions/DEC-0001.json"}


def tracked_files(root: Path) -> list[str]:
    result = subprocess.run(
        ["git", "ls-files", "-z"],
        cwd=root,
        check=True,
        capture_output=True,
    )
    return [path for path in result.stdout.decode("utf-8").split("\0") if path]


def invalid_tracked_json(
    root: Path, relative_paths: Iterable[str]
) -> list[tuple[str, str]]:
    invalid: list[tuple[str, str]] = []
    for relative_path in relative_paths:
        path = root / relative_path
        if path.suffix.lower() != ".json" or not path.is_file():
            continue
        try:
            json.loads(path.read_text(encoding="utf-8"))
        except (OSError, UnicodeError, json.JSONDecodeError) as error:
            invalid.append((relative_path, str(error)))
    return invalid


def main(root: Path = ROOT) -> int:
    missing = [path for path in REQUIRED if not (root / path).exists()]
    if missing:
        print("Missing required repository files:", *missing, sep="\n- ")
        return 1

    tracked = tracked_files(root)
    invalid_json = invalid_tracked_json(root, tracked)
    if invalid_json:
        for path, error in invalid_json:
            print(f"Invalid JSON: {path}: {error}")
        return 1

    for relative_path in INSTRUCTION_ROOTS:
        if not (root / relative_path / "AGENTS.md").exists():
            print(f"Missing nested AGENTS.md: {relative_path}")
            return 1

    legacy_name = re.compile("dene" + r"t(?!t)", re.IGNORECASE)
    legacy_paths = [path for path in tracked if legacy_name.search(path)]
    if legacy_paths:
        print("Legacy product-name paths remain:", *legacy_paths, sep="\n- ")
        return 1

    legacy_text: list[str] = []
    for relative_path in tracked:
        if relative_path in ALLOWED_LEGACY_TEXT:
            continue
        path = root / relative_path
        if not path.is_file():
            continue
        if (
            path.suffix.lower() not in TEXT_SUFFIXES
            and path.name not in SPECIAL_TEXT_FILES
        ):
            continue
        try:
            text = path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        if legacy_name.search(text):
            legacy_text.append(relative_path)
    if legacy_text:
        print("Legacy product-name identifiers remain:", *legacy_text, sep="\n- ")
        return 1

    license_files = [
        path.name
        for path in root.iterdir()
        if path.is_file() and path.name.upper().startswith("LICENSE")
    ]
    if license_files:
        print(
            "No license has been selected; remove license files:",
            *license_files,
            sep="\n- ",
        )
        return 1

    metadata_check = subprocess.run(
        [
            sys.executable,
            str(root / "tools" / "generate_repository_metadata.py"),
            "--check",
        ],
        cwd=root,
        check=False,
    )
    if metadata_check.returncode:
        return metadata_check.returncode

    print("Repository structure verification passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
