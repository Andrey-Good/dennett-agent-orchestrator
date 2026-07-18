"""Generate and verify Dennett's committed Protobuf client artifacts."""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path
import re
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
APPROVED_BUF_CONFIG_SHA256 = "c6c396e445f7d4296c2bec35ceee630878767fad405d656eacc7c3f270302609"
EPOCH_MIGRATION_MANIFEST = PROTOCOLS / "epoch-migrations" / "m00-to-m01.json"
APPROVED_EPOCH_MIGRATION_SHA256 = (
    "856dea516be7e8a36825b5934f07fd97e7f32417ee84b11a3a600225b132563f"
)
COMPARISON_BUF_CONFIG = """version: v2
modules:
  - path: proto
breaking:
  use: [WIRE_JSON]
"""
LintViolation = tuple[str, str, str]


@dataclass(frozen=True)
class EpochMigration:
    migration_id: str
    previous_epoch: str
    current_epoch: str
    base_module_sha256: str
    candidate_module_sha256: str
    retired_packages: tuple[str, ...]
    introduced_packages: tuple[str, ...]
    retired_symbol_families: tuple[str, ...]
    introduced_symbol_families: tuple[str, ...]
    decision_ref: str
    owner_gate: str


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
        body = (
            content[len(DO_NOT_EDIT_HEADER) :]
            if content.startswith(DO_NOT_EDIT_HEADER)
            else content
        )
        normalized = DO_NOT_EDIT_HEADER + body.rstrip(b"\r\n") + b"\n"
        if content != normalized:
            path.write_bytes(normalized)
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


def check_approved_buf_configuration() -> None:
    digest = hashlib.sha256((PROTOCOLS / "buf.yaml").read_bytes()).hexdigest()
    if digest != APPROVED_BUF_CONFIG_SHA256:
        raise ProtocolCheckError(
            "protocols/buf.yaml changed without an explicit checker approval; "
            f"expected {APPROVED_BUF_CONFIG_SHA256}, got {digest}"
        )
    print("Approved Buf module configuration is unchanged.")


def snapshot_protocol_module(
    source: Path,
    destination: Path,
    config: str,
) -> None:
    destination.mkdir(parents=True, exist_ok=True)
    shutil.copytree(source / "proto", destination / "proto")
    (destination / "buf.yaml").write_text(config, encoding="utf-8", newline="\n")


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
    (destination / "buf.yaml").write_text(
        COMPARISON_BUF_CONFIG,
        encoding="utf-8",
        newline="\n",
    )
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


def protocol_module_sha256(module: Path) -> str:
    digest = hashlib.sha256()
    files = sorted((module / "proto").rglob("*.proto"))
    if not files:
        raise ProtocolCheckError(f"no protocol sources found in {module}")
    for path in files:
        relative = path.relative_to(module).as_posix().encode("utf-8")
        content = path.read_bytes()
        digest.update(len(relative).to_bytes(8, "big"))
        digest.update(relative)
        digest.update(len(content).to_bytes(8, "big"))
        digest.update(content)
    return digest.hexdigest()


def protocol_packages(module: Path) -> tuple[str, ...]:
    packages: set[str] = set()
    package_pattern = re.compile(r"^package\s+([a-zA-Z0-9_.]+);$", re.MULTILINE)
    for path in sorted((module / "proto").rglob("*.proto")):
        match = package_pattern.search(path.read_text(encoding="utf-8"))
        if match is None:
            raise ProtocolCheckError(
                f"protocol source has no package declaration: {path.relative_to(module)}"
            )
        packages.add(match.group(1))
    return tuple(sorted(packages))


def protocol_epoch_changed(baseline: Path, candidate: Path) -> bool:
    return protocol_packages(baseline) != protocol_packages(candidate)


def _manifest_string_list(payload: object, field: str) -> tuple[str, ...]:
    if not isinstance(payload, list) or not payload:
        raise ProtocolCheckError(f"epoch migration field {field} must be a non-empty list")
    if not all(isinstance(item, str) and item for item in payload):
        raise ProtocolCheckError(f"epoch migration field {field} contains an invalid value")
    values = tuple(payload)
    if len(values) != len(set(values)):
        raise ProtocolCheckError(f"epoch migration field {field} contains duplicates")
    return values


def load_epoch_migration(path: Path = EPOCH_MIGRATION_MANIFEST) -> EpochMigration:
    if not path.is_file():
        raise ProtocolCheckError(f"protocol epoch migration manifest is missing: {path}")
    try:
        raw_manifest = path.read_bytes()
    except OSError as error:
        raise ProtocolCheckError(
            f"protocol epoch migration manifest is unreadable: {error}"
        ) from error
    digest = hashlib.sha256(raw_manifest).hexdigest()
    if digest != APPROVED_EPOCH_MIGRATION_SHA256:
        raise ProtocolCheckError(
            "protocol epoch migration manifest changed without checker approval; "
            f"expected {APPROVED_EPOCH_MIGRATION_SHA256}, got {digest}"
        )
    try:
        payload = json.loads(raw_manifest.decode("utf-8"))
    except (UnicodeError, json.JSONDecodeError) as error:
        raise ProtocolCheckError(
            f"protocol epoch migration manifest is unreadable: {error}"
        ) from error
    expected_fields = {
        "version",
        "migration_id",
        "previous_epoch",
        "current_epoch",
        "base_module_sha256",
        "candidate_module_sha256",
        "retired_packages",
        "introduced_packages",
        "retired_symbol_families",
        "introduced_symbol_families",
        "decision_ref",
        "owner_gate",
    }
    if not isinstance(payload, dict) or set(payload) != expected_fields:
        raise ProtocolCheckError("epoch migration manifest fields are not canonical")
    if payload["version"] != 1:
        raise ProtocolCheckError("unsupported protocol epoch migration manifest version")
    for field in (
        "migration_id",
        "previous_epoch",
        "current_epoch",
        "decision_ref",
        "owner_gate",
    ):
        if not isinstance(payload[field], str) or not payload[field]:
            raise ProtocolCheckError(f"epoch migration field {field} must be non-empty")
    for field in ("base_module_sha256", "candidate_module_sha256"):
        value = payload[field]
        if not isinstance(value, str) or re.fullmatch(r"[0-9a-f]{64}", value) is None:
            raise ProtocolCheckError(f"epoch migration field {field} is not a SHA-256")
    decision_root = (ROOT / "docs" / "decisions").resolve()
    decision_path = (ROOT / str(payload["decision_ref"])).resolve()
    if not decision_path.is_relative_to(decision_root):
        raise ProtocolCheckError(
            "epoch migration decision_ref must stay under docs/decisions"
        )
    if not decision_path.is_file():
        raise ProtocolCheckError(
            f"epoch migration decision does not exist: {payload['decision_ref']}"
        )
    return EpochMigration(
        migration_id=str(payload["migration_id"]),
        previous_epoch=str(payload["previous_epoch"]),
        current_epoch=str(payload["current_epoch"]),
        base_module_sha256=str(payload["base_module_sha256"]),
        candidate_module_sha256=str(payload["candidate_module_sha256"]),
        retired_packages=_manifest_string_list(
            payload["retired_packages"], "retired_packages"
        ),
        introduced_packages=_manifest_string_list(
            payload["introduced_packages"], "introduced_packages"
        ),
        retired_symbol_families=_manifest_string_list(
            payload["retired_symbol_families"], "retired_symbol_families"
        ),
        introduced_symbol_families=_manifest_string_list(
            payload["introduced_symbol_families"], "introduced_symbol_families"
        ),
        decision_ref=str(payload["decision_ref"]),
        owner_gate=str(payload["owner_gate"]),
    )


def epoch_migration_differences(
    baseline: Path,
    candidate: Path,
    migration: EpochMigration,
) -> list[str]:
    differences: list[str] = []
    actual_base_hash = protocol_module_sha256(baseline)
    actual_candidate_hash = protocol_module_sha256(candidate)
    base_packages = set(protocol_packages(baseline))
    candidate_packages = set(protocol_packages(candidate))
    actual_retired = tuple(sorted(base_packages - candidate_packages))
    actual_introduced = tuple(sorted(candidate_packages - base_packages))

    if migration.previous_epoch == migration.current_epoch:
        differences.append("previous and current epochs are identical")
    if actual_base_hash != migration.base_module_sha256:
        differences.append(
            f"base module hash is {actual_base_hash}, expected {migration.base_module_sha256}"
        )
    if actual_candidate_hash != migration.candidate_module_sha256:
        differences.append(
            "candidate module hash is "
            f"{actual_candidate_hash}, expected {migration.candidate_module_sha256}"
        )
    if actual_retired != tuple(sorted(migration.retired_packages)):
        differences.append(
            f"retired packages are {actual_retired}, expected {migration.retired_packages}"
        )
    if actual_introduced != tuple(sorted(migration.introduced_packages)):
        differences.append(
            "introduced packages are "
            f"{actual_introduced}, expected {migration.introduced_packages}"
        )
    return differences


def _normalise_lint_path(raw_path: str) -> str:
    path = raw_path.replace("\\", "/")
    if path.startswith("proto/"):
        return path
    marker = "/proto/"
    if marker in path:
        return f"proto/{path.split(marker, 1)[1]}"
    raise ProtocolCheckError(f"strict Buf lint returned an unexpected path: {raw_path}")


def parse_lint_violations(
    result: subprocess.CompletedProcess[bytes],
) -> frozenset[LintViolation]:
    violations: set[LintViolation] = set()
    output = b"\n".join(stream for stream in (result.stdout, result.stderr) if stream)
    for line in output.decode("utf-8", errors="replace").splitlines():
        if not line.strip():
            continue
        try:
            payload = json.loads(line)
            violations.add(
                (
                    _normalise_lint_path(str(payload["path"])),
                    str(payload["type"]),
                    str(payload["message"]),
                )
            )
        except (json.JSONDecodeError, KeyError, TypeError) as error:
            raise ProtocolCheckError(
                f"strict Buf lint returned non-violation output: {line}"
            ) from error
    if result.returncode != 0 and not violations:
        raise ProtocolCheckError("strict Buf lint failed without structured violations")
    return frozenset(violations)


def check_strict_standard_lint() -> None:
    run(["buf", "lint", str(PROTOCOLS)])
    print("Buf STANDARD lint passed with no ignores or grandfathered findings.")


def check_negative_lint_probe() -> None:
    with TemporaryDirectory(prefix="dennett-protocol-lint-probe-") as directory:
        candidate = Path(directory) / "protocols"
        shutil.copytree(PROTOCOLS, candidate)
        target = (
            candidate
            / "proto"
            / "dennett"
            / "control"
            / "v1"
            / "session.proto"
        )
        with target.open("a", encoding="utf-8", newline="\n") as stream:
            stream.write(
                "\nmessage ProbeRequest {}\n"
                "message ProbeResponse {}\n"
                "service NewLegacy {\n"
                "  rpc Probe(ProbeRequest) returns (ProbeResponse);\n"
                "}\n"
            )
        run(["buf", "build", str(candidate)])
        result = run(
            ["buf", "lint", str(candidate), "--error-format", "json"],
            capture_output=True,
            check=False,
        )
        violations = parse_lint_violations(result)
        expected = (
            "proto/dennett/control/v1/session.proto",
            "SERVICE_SUFFIX",
        )
        if result.returncode == 0 or not any(
            (path, rule) == expected for path, rule, _message in violations
        ):
            raise ProtocolCheckError(
                "strict lint negative probe accepted a service without the required suffix"
            )
    print("Negative lint probe rejected a newly introduced STANDARD violation.")


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
    migration_used: EpochMigration | None = None
    with TemporaryDirectory(prefix="dennett-protocol-baseline-") as directory:
        root = Path(directory)
        baseline = root / "baseline"
        candidate = root / "candidate"
        extract_protocol_baseline(base_ref, baseline)
        snapshot_protocol_module(PROTOCOLS, candidate, COMPARISON_BUF_CONFIG)
        result = run(
            ["buf", "breaking", str(candidate), "--against", str(baseline)],
            capture_output=True,
            check=False,
        )
        package_epoch_changed = protocol_epoch_changed(baseline, candidate)
        if package_epoch_changed:
            migration = load_epoch_migration()
            differences = epoch_migration_differences(baseline, candidate, migration)
            if differences:
                _print_process_output(result)
                details = "\n".join(f"- {difference}" for difference in differences)
                raise ProtocolCheckError(
                    "breaking protocol change does not match the approved epoch "
                    f"migration {migration.migration_id}:\n{details}"
                )
            migration_used = migration
        elif result.returncode != 0:
            _print_process_output(result)
            raise ProtocolCheckError(
                f"protocol compatibility failed against {base_ref} ({commit[:12]})"
            )
    if migration_used is None:
        print(f"Protocol compatibility passed against {base_ref} ({commit[:12]}).")
    else:
        print(
            f"Protocol epoch migration {migration_used.migration_id} exactly matches "
            f"{base_ref} ({commit[:12]}); owner gate {migration_used.owner_gate} remains."
        )


def check_negative_breaking_probe() -> None:
    with TemporaryDirectory(prefix="dennett-protocol-breaking-") as directory:
        root = Path(directory)
        baseline = root / "baseline"
        candidate = root / "candidate"
        snapshot_protocol_module(PROTOCOLS, baseline, COMPARISON_BUF_CONFIG)
        snapshot_protocol_module(PROTOCOLS, candidate, COMPARISON_BUF_CONFIG)
        target = (
            candidate
            / "proto"
            / "dennett"
            / "common"
            / "v1"
            / "common.proto"
        )
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
        run(["buf", "build", str(candidate)])
        result = run(
            ["buf", "breaking", str(candidate), "--against", str(baseline)],
            capture_output=True,
            check=False,
        )
        if result.returncode == 0:
            raise ProtocolCheckError(
                "Buf accepted an incompatible field-type reuse in the negative probe"
            )
    print("Negative compatibility probe rejected incompatible field-number reuse.")


def check(explicit_base_ref: str | None) -> None:
    check_approved_buf_configuration()
    check_strict_standard_lint()
    check_negative_lint_probe()
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
