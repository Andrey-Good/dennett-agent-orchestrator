"""Generate and verify Dennett's committed Protobuf client artifacts."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys
from tempfile import TemporaryDirectory
from typing import Mapping, Sequence


ROOT = Path(__file__).resolve().parents[1]
PROTOCOLS = ROOT / "protocols"
GENERATED = ROOT / "generated"
GENERATOR_TEMPLATE = PROTOCOLS / "buf.gen.yaml"
DO_NOT_EDIT_HEADER = b"// DO NOT EDIT. Generated from protocols/proto by Buf.\n"
GENERATED_LANGUAGES = ("rust", "ts")


class ProtocolCheckError(RuntimeError):
    """A protocol contract check failed with an actionable message."""


def run(
    command: Sequence[str],
    *,
    cwd: Path = ROOT,
    capture_output: bool = False,
    check: bool = True,
    announce: bool = True,
) -> subprocess.CompletedProcess[bytes]:
    if announce:
        print(f"+ {subprocess.list2cmdline(command)}", flush=True)
    executable = shutil.which(command[0])
    if executable is None:
        raise FileNotFoundError(command[0])
    result = subprocess.run(
        [executable, *command[1:]],
        cwd=cwd,
        check=False,
        capture_output=capture_output,
    )
    if check and result.returncode != 0:
        if capture_output:
            _print_process_output(result)
        raise subprocess.CalledProcessError(
            result.returncode,
            command,
            output=result.stdout,
            stderr=result.stderr,
        )
    return result


def _print_process_output(result: subprocess.CompletedProcess[bytes]) -> None:
    for stream in (result.stdout, result.stderr):
        if stream:
            print(stream.decode("utf-8", errors="replace"), file=sys.stderr, end="")


def proto_files(module: Path = PROTOCOLS) -> list[Path]:
    return sorted((module / "proto").rglob("*.proto"))


def add_do_not_edit_headers(root: Path) -> list[Path]:
    changed: list[Path] = []
    for path in sorted(root.rglob("*")):
        if not path.is_file() or path.suffix not in {".rs", ".ts"}:
            continue
        content = path.read_bytes()
        if not content.startswith(DO_NOT_EDIT_HEADER):
            path.write_bytes(DO_NOT_EDIT_HEADER + content)
            changed.append(path)
    return changed


def tree_differences(actual: Path, expected: Path, label: str) -> list[str]:
    actual_files = _relative_files(actual)
    expected_files = _relative_files(expected)
    differences: list[str] = []
    for relative in sorted(actual_files.keys() | expected_files.keys()):
        display = f"{label}/{relative.as_posix()}"
        if relative not in actual_files:
            differences.append(f"missing: {display}")
        elif relative not in expected_files:
            differences.append(f"unexpected: {display}")
        elif actual_files[relative] != expected_files[relative]:
            differences.append(f"stale: {display}")
    return differences


def _relative_files(root: Path) -> dict[Path, bytes]:
    if not root.exists():
        return {}
    return {
        path.relative_to(root): path.read_bytes()
        for path in sorted(root.rglob("*"))
        if path.is_file()
    }


def base_ref_candidates(
    explicit: str | None,
    environment: Mapping[str, str] = os.environ,
) -> list[str]:
    if explicit:
        return [explicit]
    github_base = environment.get("GITHUB_BASE_REF", "").strip()
    if github_base:
        return [f"origin/{github_base}", github_base]
    return ["origin/main", "main"]


def resolve_base_ref(explicit: str | None) -> tuple[str, str]:
    for candidate in base_ref_candidates(explicit):
        result = run(
            ["git", "rev-parse", "--verify", "--quiet", f"{candidate}^{{commit}}"],
            capture_output=True,
            check=False,
            announce=False,
        )
        if result.returncode == 0:
            return candidate, result.stdout.decode("ascii").strip()
    attempted = ", ".join(base_ref_candidates(explicit))
    raise ProtocolCheckError(f"cannot resolve protocol comparison base; tried: {attempted}")


def extract_protocol_baseline(ref: str, destination: Path) -> None:
    listing = run(
        [
            "git",
            "ls-tree",
            "-r",
            "--name-only",
            "-z",
            ref,
            "--",
            "protocols/proto",
        ],
        capture_output=True,
        announce=False,
    )
    paths = [
        path
        for path in listing.stdout.decode("utf-8").split("\0")
        if path.endswith(".proto")
    ]
    if not paths:
        raise ProtocolCheckError(f"no canonical .proto files found at {ref}")

    destination.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(PROTOCOLS / "buf.yaml", destination / "buf.yaml")
    for repository_path in paths:
        relative = Path(repository_path).relative_to("protocols")
        target = destination / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        content = run(
            ["git", "show", f"{ref}:{repository_path}"],
            capture_output=True,
            announce=False,
        ).stdout
        target.write_bytes(content)


def generate_into(cwd: Path) -> Path:
    run(
        [
            "buf",
            "generate",
            str(PROTOCOLS),
            "--template",
            str(GENERATOR_TEMPLATE),
        ],
        cwd=cwd,
    )
    output = cwd / "generated"
    add_do_not_edit_headers(output)
    return output


def generate() -> None:
    generate_into(ROOT)
    files = sum(len(_relative_files(GENERATED / language)) for language in GENERATED_LANGUAGES)
    print(f"Generated {files} committed protocol client artifacts.")


def check_format() -> None:
    stale: list[str] = []
    files = proto_files()
    if not files:
        raise ProtocolCheckError("no canonical Protobuf sources found under protocols/proto")
    for path in files:
        formatted = run(
            ["buf", "format", str(path.relative_to(ROOT))],
            capture_output=True,
            announce=False,
        ).stdout
        if formatted != path.read_bytes():
            stale.append(path.relative_to(ROOT).as_posix())
    if stale:
        details = "\n".join(f"- {path}" for path in stale)
        raise ProtocolCheckError(f"stale protocol formatting:\n{details}")
    print(f"Buf format check passed ({len(files)} files).")


def check_generated() -> None:
    with TemporaryDirectory(prefix="dennett-protocol-generation-") as directory:
        expected = generate_into(Path(directory))
        differences: list[str] = []
        for language in GENERATED_LANGUAGES:
            differences.extend(
                tree_differences(
                    GENERATED / language,
                    expected / language,
                    f"generated/{language}",
                )
            )
    if differences:
        details = "\n".join(f"- {difference}" for difference in differences)
        raise ProtocolCheckError(f"generated protocol artifacts are not current:\n{details}")
    print("Committed Rust and TypeScript protocol artifacts are current.")


def check_against_main(explicit_base_ref: str | None) -> None:
    base_ref, commit = resolve_base_ref(explicit_base_ref)
    with TemporaryDirectory(prefix="dennett-protocol-baseline-") as directory:
        baseline = Path(directory) / "protocols"
        extract_protocol_baseline(base_ref, baseline)
        run(["buf", "breaking", str(PROTOCOLS), "--against", str(baseline)])
    print(f"Protocol compatibility passed against {base_ref} ({commit[:12]}).")


def check_negative_breaking_probe() -> None:
    with TemporaryDirectory(prefix="dennett-protocol-breaking-") as directory:
        candidate = Path(directory) / "protocols"
        shutil.copytree(PROTOCOLS, candidate)
        target = candidate / "proto" / "dennett" / "v1" / "common.proto"
        content = target.read_text(encoding="utf-8")
        original = "string kind = 1;"
        incompatible = "int64 kind = 1;"
        if content.count(original) != 1:
            raise ProtocolCheckError(
                "negative compatibility fixture cannot find StableRef.kind field"
            )
        target.write_text(
            content.replace(original, incompatible, 1),
            encoding="utf-8",
            newline="\n",
        )
        run(["buf", "lint", str(candidate)])
        result = run(
            ["buf", "breaking", str(candidate), "--against", str(PROTOCOLS)],
            capture_output=True,
            check=False,
        )
        if result.returncode == 0:
            raise ProtocolCheckError(
                "Buf accepted an incompatible field-type reuse in the negative probe"
            )
    print("Negative compatibility probe rejected incompatible field-number reuse.")


def check(explicit_base_ref: str | None) -> None:
    run(["buf", "lint", str(PROTOCOLS)])
    check_format()
    check_generated()
    check_against_main(explicit_base_ref)
    check_negative_breaking_probe()
    print("Protocol contract checks passed.")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    subcommands = parser.add_subparsers(dest="command", required=True)
    subcommands.add_parser("generate", help="regenerate committed clients")
    check_parser = subcommands.add_parser("check", help="run protocol contract gates")
    check_parser.add_argument(
        "--base-ref",
        help="Git ref to compare against (defaults to the PR base or main)",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.command == "generate":
        generate()
    else:
        check(args.base_ref)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except ProtocolCheckError as error:
        print(f"protocol check failed: {error}", file=sys.stderr)
        raise SystemExit(1) from error
    except FileNotFoundError as error:
        missing = error.filename or (error.args[0] if error.args else "unknown")
        print(f"required protocol tool is missing: {missing}", file=sys.stderr)
        raise SystemExit(1) from error
    except subprocess.CalledProcessError as error:
        print(
            f"protocol command failed with exit code {error.returncode}: "
            f"{subprocess.list2cmdline(error.cmd)}",
            file=sys.stderr,
        )
        raise SystemExit(error.returncode) from error
