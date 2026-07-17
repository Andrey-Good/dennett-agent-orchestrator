
from __future__ import annotations
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
required = [
    "README.md", "AGENTS.md", "Cargo.toml",
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
missing = [p for p in required if not (ROOT / p).exists()]
if missing:
    print("Missing required repository files:", *missing, sep="\n- ")
    sys.exit(1)

# Validate JSON files.
for p in ROOT.rglob("*.json"):
    try:
        json.loads(p.read_text(encoding="utf-8"))
    except Exception as exc:
        print(f"Invalid JSON: {p.relative_to(ROOT)}: {exc}")
        sys.exit(1)

# Major bounded roots need instructions.
for p in [
    "apps/desktop", "apps/mobile", "services/head", "services/node",
    "services/memoryd", "services/sensor-worker", "crates/dennett-memory-core",
    "crates/dennett-trust-core", "adapters", "protocols", "tests/scenarios"
]:
    if not (ROOT / p / "AGENTS.md").exists():
        print(f"Missing nested AGENTS.md: {p}")
        sys.exit(1)

# Canonical product identity is an owner decision for M00.
tracked_result = subprocess.run(
    ["git", "ls-files", "-z"],
    cwd=ROOT,
    check=True,
    capture_output=True,
)
tracked = [
    path
    for path in tracked_result.stdout.decode("utf-8").split("\0")
    if path
]
legacy_name = re.compile("dene" + r"t(?!t)", re.IGNORECASE)
legacy_paths = [path for path in tracked if legacy_name.search(path)]
if legacy_paths:
    print("Legacy product-name paths remain:", *legacy_paths, sep="\n- ")
    sys.exit(1)

allowed_legacy_text = {"planning/decisions/DEC-0001.json"}
text_suffixes = {
    ".json", ".md", ".proto", ".py", ".rs", ".toml", ".ts", ".tsx",
    ".txt", ".yaml", ".yml",
}
legacy_text: list[str] = []
for relative_path in tracked:
    if relative_path in allowed_legacy_text:
        continue
    path = ROOT / relative_path
    if not path.is_file():
        continue
    if path.suffix.lower() not in text_suffixes and path.name not in {
        ".env.example",
        ".gitattributes",
        ".gitignore",
        "Justfile",
    }:
        continue
    try:
        text = path.read_text(encoding="utf-8")
    except UnicodeDecodeError:
        continue
    if legacy_name.search(text):
        legacy_text.append(relative_path)
if legacy_text:
    print("Legacy product-name identifiers remain:", *legacy_text, sep="\n- ")
    sys.exit(1)

license_files = [
    path.name
    for path in ROOT.iterdir()
    if path.is_file() and path.name.upper().startswith("LICENSE")
]
if license_files:
    print("No license has been selected; remove license files:", *license_files, sep="\n- ")
    sys.exit(1)

metadata_check = subprocess.run(
    [sys.executable, str(ROOT / "tools" / "generate_repository_metadata.py"), "--check"],
    cwd=ROOT,
)
if metadata_check.returncode:
    sys.exit(metadata_check.returncode)

print("Repository structure verification passed.")
