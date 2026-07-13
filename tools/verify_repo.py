
from __future__ import annotations
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
required = [
    "README.md", "AGENTS.md", "Cargo.toml",
    "docs/specifications/00_Denet_Functional_Concept.md",
    "docs/architecture/80_Denet_System_Architecture_and_Runtime_Topology.md",
    "docs/architecture/83_Denet_Client_Operations_Testing_and_Implementation_Blueprint.md",
    "crates/denet-contracts/src/lib.rs",
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
    "services/memoryd", "services/sensor-worker", "crates/denet-memory-core",
    "crates/denet-trust-core", "adapters", "protocols", "tests/scenarios"
]:
    if not (ROOT / p / "AGENTS.md").exists():
        print(f"Missing nested AGENTS.md: {p}")
        sys.exit(1)

print("Repository structure verification passed.")
