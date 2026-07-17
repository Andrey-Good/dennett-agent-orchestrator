"""Validate the structured GitHub Fast Gate and branch-protection contract."""

from __future__ import annotations

import ast
import json
from pathlib import Path
import sys
from typing import Any

import yaml


ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = Path(".github/workflows/ci.yml")
PROTECTION = Path(".github/branch-protection.main.json")
JUSTFILE = Path("Justfile")
BOOTSTRAP = Path("tools/bootstrap.py")
CHECKOUT = "actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5"
MISE_ACTION = "jdx/mise-action@5228313ee0372e111a38da051671ca30fc5a96db"
CLEAN_COMMAND = "uv run --project . --frozen python tools/verify_worktree_clean.py"
EXPECTED_STEPS = [
    {"uses": CHECKOUT, "with": {"fetch-depth": 0}},
    {
        "uses": MISE_ACTION,
        "with": {"version": "2026.7.7", "cache": True},
    },
    {
        "name": "Bootstrap pinned tools and frozen dependencies",
        "run": "just bootstrap",
    },
    {
        "name": "Reject generated, lockfile or untracked drift from bootstrap",
        "run": CLEAN_COMMAND,
    },
    {"name": "Run complete fast gate", "run": "just check"},
    {"name": "Reject verification drift", "run": CLEAN_COMMAND},
]
EXPECTED_RECIPES = {
    "bootstrap": (
        [],
        [
            "mise install",
            "mise exec -- uv python install 3.13.5",
            "mise exec -- uv sync --project . --frozen",
            "mise exec -- uv run --project . --frozen python tools/bootstrap.py",
        ],
    ),
    "verify": (
        [],
        [
            "mise exec -- uv run --project . --frozen python tools/verify_repo.py",
            "mise exec -- uv run --project . --frozen python tools/verify_docs.py",
            "mise exec -- uv run --project . --frozen python tools/verify_planning.py",
            "mise exec -- uv run --project . --frozen python tools/verify_ci.py",
            "mise exec -- uv run --project . --frozen python tools/generate_test_catalogue.py --check",
            "mise exec -- uv run --project . --frozen python tools/generate_doc_index.py --check",
            "mise exec -- uv run --project . --frozen python tools/generate_repository_metadata.py --check",
        ],
    ),
    "test-contracts": (
        [],
        [
            "mise exec -- uv run --project . --frozen python tools/protocol_codegen.py check"
        ],
    ),
    "rust": (
        [],
        [
            "mise exec -- uv run --project . --frozen python tools/run_in_toolchain.py cargo fmt --check",
            "mise exec -- uv run --project . --frozen python tools/run_in_toolchain.py cargo clippy --workspace --all-targets -- -D warnings",
            "mise exec -- uv run --project . --frozen python tools/run_in_toolchain.py cargo test --workspace",
        ],
    ),
    "python": (
        [],
        [
            "mise exec -- uv run --project . --frozen python -m unittest discover -s services/adapter-host-python/tests",
            "mise exec -- uv run --project . --frozen python -m unittest discover -s tools/tests",
        ],
    ),
    "ts": ([], ["mise exec -- corepack pnpm typecheck"]),
    "check": (["verify", "rust", "python", "ts", "test-contracts"], []),
}
EXPECTED_BOOTSTRAP_COMMANDS = {
    ("corepack", "install"),
    ("corepack", "pnpm", "install", "--frozen-lockfile"),
    ("cargo", "fetch", "--locked"),
    ("cargo", "fmt", "--version"),
    ("cargo", "clippy", "--version"),
    ("<python>", "tools/protocol_codegen.py", "generate"),
}
EXPECTED_PROTECTION = {
    "branch": "main",
    "required_status_checks": {"strict": True, "contexts": ["Fast Gate"]},
    "enforce_admins": True,
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


def _read(root: Path, relative: Path, diagnostics: list[str]) -> str | None:
    path = root / relative
    if not path.is_file():
        diagnostics.append(f"{relative.as_posix()}: missing required CI file")
        return None
    return path.read_text(encoding="utf-8")


def _mapping(value: Any) -> dict[Any, Any]:
    return value if isinstance(value, dict) else {}


def _validate_workflow(text: str, diagnostics: list[str]) -> None:
    try:
        document = yaml.safe_load(text)
    except yaml.YAMLError as error:
        diagnostics.append(f"{WORKFLOW.as_posix()}: invalid YAML: {error}")
        return
    if not isinstance(document, dict):
        diagnostics.append(f"{WORKFLOW.as_posix()}: workflow root must be a mapping")
        return

    if document.get("name") != "Fast Gate":
        diagnostics.append(f"{WORKFLOW.as_posix()}: workflow name must be Fast Gate")
    triggers = document.get("on", document.get(True))
    trigger_map = _mapping(triggers)
    if set(trigger_map) != {"pull_request", "push", "merge_group"}:
        diagnostics.append(
            f"{WORKFLOW.as_posix()}: triggers must be pull_request, push and merge_group"
        )
    if _mapping(trigger_map.get("push")).get("branches") != ["main"]:
        diagnostics.append(f"{WORKFLOW.as_posix()}: push trigger must target main")
    if document.get("permissions") != {"contents": "read"}:
        diagnostics.append(f"{WORKFLOW.as_posix()}: permissions must be contents: read")
    expected_concurrency = {
        "group": "fast-gate-${{ github.workflow }}-${{ github.event.pull_request.number || github.ref }}",
        "cancel-in-progress": True,
    }
    if document.get("concurrency") != expected_concurrency:
        diagnostics.append(f"{WORKFLOW.as_posix()}: concurrency contract differs")

    jobs = _mapping(document.get("jobs"))
    if set(jobs) != {"fast-gate"}:
        diagnostics.append(f"{WORKFLOW.as_posix()}: exactly one fast-gate job is required")
        return
    job = _mapping(jobs.get("fast-gate"))
    expected_job_values = {
        "name": "Fast Gate",
        "runs-on": "ubuntu-24.04",
        "timeout-minutes": 10,
    }
    for key, expected in expected_job_values.items():
        if job.get(key) != expected:
            diagnostics.append(
                f"{WORKFLOW.as_posix()}: fast-gate {key} must be {expected!r}"
            )
    if job.get("steps") != EXPECTED_STEPS:
        diagnostics.append(
            f"{WORKFLOW.as_posix()}: fast-gate steps differ from the approved sequence"
        )
    unexpected_job_keys = set(job) - {"name", "runs-on", "timeout-minutes", "steps"}
    if unexpected_job_keys:
        diagnostics.append(
            f"{WORKFLOW.as_posix()}: unexpected fast-gate keys {sorted(unexpected_job_keys)!r}"
        )


def _recipe(justfile: str, name: str) -> tuple[list[str], list[str]] | None:
    lines = justfile.splitlines()
    prefix = f"{name}:"
    for index, line in enumerate(lines):
        if not line.startswith(prefix):
            continue
        dependencies = line.removeprefix(prefix).strip().split()
        body: list[str] = []
        for candidate in lines[index + 1 :]:
            if not candidate.strip():
                continue
            if not candidate.startswith((" ", "\t")):
                break
            body.append(candidate.strip())
        return dependencies, body
    return None


def _validate_justfile(text: str, diagnostics: list[str]) -> None:
    for name, expected in EXPECTED_RECIPES.items():
        observed = _recipe(text, name)
        if observed != expected:
            diagnostics.append(
                f"{JUSTFILE.as_posix()}: recipe {name} differs from the approved Fast Gate contract"
            )


def _command_value(node: ast.AST) -> str | None:
    if isinstance(node, ast.Constant) and isinstance(node.value, str):
        return node.value
    if (
        isinstance(node, ast.Attribute)
        and isinstance(node.value, ast.Name)
        and node.value.id == "sys"
        and node.attr == "executable"
    ):
        return "<python>"
    return None


def _bootstrap_commands(text: str) -> set[tuple[str, ...]]:
    tree = ast.parse(text)
    commands: set[tuple[str, ...]] = set()
    for node in ast.walk(tree):
        if not isinstance(node, ast.Call) or not isinstance(node.func, ast.Name):
            continue
        if node.func.id != "run" or not node.args:
            continue
        argument = node.args[0]
        if not isinstance(argument, (ast.List, ast.Tuple)):
            continue
        values = tuple(_command_value(element) for element in argument.elts)
        if all(value is not None for value in values):
            commands.add(tuple(value for value in values if value is not None))
    return commands


def _validate_bootstrap(text: str, diagnostics: list[str]) -> None:
    try:
        observed = _bootstrap_commands(text)
    except SyntaxError as error:
        diagnostics.append(f"{BOOTSTRAP.as_posix()}: invalid Python: {error}")
        return
    if not EXPECTED_BOOTSTRAP_COMMANDS.issubset(observed):
        missing = sorted(EXPECTED_BOOTSTRAP_COMMANDS - observed)
        diagnostics.append(
            f"{BOOTSTRAP.as_posix()}: frozen bootstrap commands missing {missing!r}"
        )


def validate(root: Path = ROOT) -> list[str]:
    diagnostics: list[str] = []
    workflow = _read(root, WORKFLOW, diagnostics)
    protection_text = _read(root, PROTECTION, diagnostics)
    justfile = _read(root, JUSTFILE, diagnostics)
    bootstrap = _read(root, BOOTSTRAP, diagnostics)

    if workflow is not None:
        _validate_workflow(workflow, diagnostics)
    if protection_text is not None:
        try:
            protection = json.loads(protection_text)
        except json.JSONDecodeError as error:
            diagnostics.append(f"{PROTECTION.as_posix()}: invalid JSON: {error}")
        else:
            if protection != EXPECTED_PROTECTION:
                diagnostics.append(
                    f"{PROTECTION.as_posix()}: configuration differs from the approved contract"
                )
    if justfile is not None:
        _validate_justfile(justfile, diagnostics)
    if bootstrap is not None:
        _validate_bootstrap(bootstrap, diagnostics)
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
