"""Validate the exact development tool versions declared by the repository."""

from __future__ import annotations

from dataclasses import dataclass
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tomllib
from typing import Callable, Sequence


ROOT = Path(__file__).resolve().parents[1]
Runner = Callable[[Sequence[str]], str]


@dataclass(frozen=True)
class Probe:
    name: str
    expected: str
    command: tuple[str, ...]
    pattern: str


@dataclass(frozen=True)
class ProbeResult:
    name: str
    expected: str
    actual: str
    ok: bool


def expected_versions(root: Path = ROOT) -> dict[str, str]:
    mise = tomllib.loads((root / "mise.toml").read_text(encoding="utf-8"))["tools"]
    package = json.loads((root / "package.json").read_text(encoding="utf-8"))
    project = tomllib.loads((root / "pyproject.toml").read_text(encoding="utf-8"))

    rust = mise["rust"]
    rust_version = rust["version"] if isinstance(rust, dict) else rust
    python_spec = project["project"]["requires-python"]
    if not python_spec.startswith("=="):
        raise ValueError("project.requires-python must pin an exact Python version")

    return {
        "buf": str(mise["buf"]),
        "cargo": str(rust_version),
        "just": str(mise["just"]),
        "node": str(mise["node"]),
        "pnpm": package["packageManager"].split("@", maxsplit=1)[1],
        "protoc": str(mise["protoc"]),
        "python": python_spec.removeprefix("=="),
        "rustc": str(rust_version),
        "uv": str(mise["uv"]),
    }


def probes(expected: dict[str, str]) -> list[Probe]:
    return [
        Probe("buf", expected["buf"], ("buf", "--version"), r"^(\S+)$"),
        Probe("cargo", expected["cargo"], ("cargo", "--version"), r"^cargo (\S+)"),
        Probe("just", expected["just"], ("just", "--version"), r"^just (\S+)"),
        Probe("node", expected["node"], ("node", "--version"), r"^v(\S+)$"),
        Probe(
            "pnpm",
            expected["pnpm"],
            ("corepack", "pnpm", "--version"),
            r"^(\S+)$",
        ),
        Probe(
            "protoc",
            expected["protoc"],
            ("protoc", "--version"),
            r"^libprotoc (\S+)$",
        ),
        Probe(
            "python",
            expected["python"],
            ("uv", "run", "--project", ".", "--frozen", "python", "--version"),
            r"^Python (\S+)$",
        ),
        Probe("rustc", expected["rustc"], ("rustc", "--version"), r"^rustc (\S+)"),
        Probe("uv", expected["uv"], ("uv", "--version"), r"^uv (\S+)"),
    ]


def run_command(command: Sequence[str]) -> str:
    executable = shutil.which(command[0])
    if executable is None:
        raise FileNotFoundError(command[0])
    completed = subprocess.run(
        [executable, *command[1:]],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
        timeout=30,
    )
    return (completed.stdout or completed.stderr).strip()


def collect_results(
    expected: dict[str, str], runner: Runner = run_command
) -> list[ProbeResult]:
    results = []
    for probe in probes(expected):
        try:
            output = runner(probe.command)
            match = re.search(probe.pattern, output)
            actual = match.group(1) if match else f"unrecognized: {output}"
        except (FileNotFoundError, subprocess.CalledProcessError, subprocess.TimeoutExpired) as error:
            actual = f"unavailable: {error}"
        results.append(
            ProbeResult(probe.name, probe.expected, actual, actual == probe.expected)
        )
    return results


def main() -> int:
    try:
        expected = expected_versions()
    except (OSError, KeyError, ValueError, tomllib.TOMLDecodeError) as error:
        print(f"doctor could not read repository pins: {error}", file=sys.stderr)
        return 2

    results = collect_results(expected)
    print("tool       expected     actual       status")
    for result in results:
        status = "ok" if result.ok else "MISMATCH"
        print(
            f"{result.name:10} {result.expected:12} {result.actual:12} {status}"
        )
    return 0 if all(result.ok for result in results) else 1


if __name__ == "__main__":
    raise SystemExit(main())
