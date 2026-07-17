"""Fail when a verification command leaves tracked or untracked repository drift."""

from __future__ import annotations

from pathlib import Path
import subprocess
import sys
from typing import Callable


ROOT = Path(__file__).resolve().parents[1]
Runner = Callable[..., subprocess.CompletedProcess[str]]


def changed_entries(root: Path = ROOT, runner: Runner = subprocess.run) -> list[str]:
    completed = runner(
        ["git", "status", "--porcelain=v1", "--untracked-files=all"],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    )
    return [line for line in completed.stdout.splitlines() if line]


def main() -> int:
    entries = changed_entries()
    if entries:
        print("Verification left repository drift:", file=sys.stderr)
        for entry in entries:
            print(f"- {entry}", file=sys.stderr)
        return 1
    print("Verification worktree is clean (tracked and untracked files).")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
