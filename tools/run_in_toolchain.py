"""Run a development command with the native compiler environment when needed."""

from __future__ import annotations

import os
from pathlib import Path
import shutil
import subprocess
import sys


def _vs_dev_cmd() -> Path | None:
    if os.name != "nt":
        return None

    candidates = []
    for variable in ("ProgramFiles(x86)", "ProgramFiles"):
        base = os.environ.get(variable)
        if base:
            candidates.extend(
                Path(base).glob(
                    "Microsoft Visual Studio/*/*/Common7/Tools/VsDevCmd.bat"
                )
            )
    return sorted(candidates, reverse=True)[0] if candidates else None


def run(command: list[str]) -> int:
    if not command:
        raise ValueError("a command is required")

    if os.name != "nt" or command[0] not in {"cargo", "rustc"}:
        return subprocess.run(command, check=False).returncode

    if shutil.which("link.exe"):
        return subprocess.run(command, check=False).returncode

    vs_dev_cmd = _vs_dev_cmd()
    if vs_dev_cmd is None:
        print(
            "MSVC Build Tools are required for the Windows Rust target; "
            "install the Desktop development with C++ workload.",
            file=sys.stderr,
        )
        return 2

    environment = os.environ.copy()
    setup = subprocess.run(
        f'call "{vs_dev_cmd}" -no_logo -arch=x64 -host_arch=x64 >nul && set',
        shell=True,
        check=True,
        capture_output=True,
        text=True,
    )
    for line in setup.stdout.splitlines():
        name, separator, value = line.partition("=")
        if separator:
            environment[name] = value
    return subprocess.run(command, check=False, env=environment).returncode


def main() -> int:
    return run(sys.argv[1:])


if __name__ == "__main__":
    raise SystemExit(main())
