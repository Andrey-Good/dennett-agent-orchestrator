"""Finish repository bootstrap after mise and uv have installed pinned runtimes."""

from __future__ import annotations

import os
from pathlib import Path
import shutil
import subprocess
import sys

if __package__:
    from tools import doctor
else:
    import doctor


ROOT = Path(__file__).resolve().parents[1]


def run(command: list[str]) -> None:
    print(f"+ {' '.join(command)}", flush=True)
    executable = shutil.which(command[0])
    if executable is None:
        raise FileNotFoundError(command[0])
    environment = {**os.environ, "CI": "true"}
    subprocess.run(
        [executable, *command[1:]], cwd=ROOT, check=True, env=environment
    )


def create_local_config(root: Path = ROOT) -> None:
    source = root / ".env.example"
    destination = root / ".env"
    if not destination.exists():
        shutil.copyfile(source, destination)
        print("created .env from .env.example (local and git-ignored)")


def main() -> int:
    run(["corepack", "install"])
    run(["corepack", "pnpm", "install", "--frozen-lockfile"])
    run(["cargo", "fetch", "--locked"])
    run(["cargo", "fmt", "--version"])
    run(["cargo", "clippy", "--version"])
    run([sys.executable, "tools/protocol_codegen.py", "generate"])
    create_local_config()
    return doctor.main()


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except subprocess.CalledProcessError as error:
        print(f"bootstrap command failed with exit code {error.returncode}", file=sys.stderr)
        raise SystemExit(error.returncode) from error
