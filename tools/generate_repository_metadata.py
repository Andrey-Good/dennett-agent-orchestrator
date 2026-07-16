from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REPOSITORY_EDITION = "2026-07-16.m00"
ROOT_MANIFEST = ROOT / "REPOSITORY_MANIFEST.json"
ROOT_CHECKSUMS = ROOT / "REPOSITORY_CHECKSUMS.sha256"
DOCS_MANIFEST = ROOT / "docs" / "manifest.json"
DOCS_CHECKSUMS = ROOT / "docs" / "CHECKSUMS.sha256"


def repository_paths() -> list[Path]:
    result = subprocess.run(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"],
        cwd=ROOT,
        check=True,
        capture_output=True,
    )
    paths = []
    for raw_path in result.stdout.decode("utf-8").split("\0"):
        if not raw_path:
            continue
        path = ROOT / raw_path
        if path.is_file():
            paths.append(path)
    return sorted(set(paths), key=lambda path: path.relative_to(ROOT).as_posix())


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def relative(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def render_docs_manifest() -> str:
    specifications = sorted(
        path.name
        for path in (ROOT / "docs" / "specifications").glob("[0-9][0-9]_Dennett_*.md")
    )
    architecture = sorted(
        path.name
        for path in (ROOT / "docs" / "architecture").glob("8[0-3]_Dennett_*.md")
    )
    contracts = sorted(
        path.name
        for path in (ROOT / "docs" / "specifications" / "contracts").glob("*.md")
        if path.name not in {"README.md", "REFERENCES.md"}
    )
    implementation = sorted(
        path.name
        for path in (ROOT / "docs" / "implementation").glob("0[0-4]_*.md")
    )
    testing = sorted(path.name for path in (ROOT / "docs" / "testing").glob("*.md"))
    payload = {
        "repository_edition": REPOSITORY_EDITION,
        "canonical_specifications": specifications,
        "architecture_volumes": architecture,
        "contracts": contracts,
        "head_policy": {
            "default": "none",
            "allowed": ["none", "emergency", "full"],
            "user_opt_in_required": True,
        },
        "memory_policy": "one logical Memory Fabric; physical role varies by deployment",
        "implementation_documents": implementation,
        "testing_documents": testing,
        "planning_policy": (
            "semantic code changes require a bounded Work Package; "
            "autonomous work requires a batch envelope"
        ),
    }
    return json.dumps(payload, ensure_ascii=False, indent=2) + "\n"


def render_checksums(paths: list[Path]) -> str:
    return "".join(f"{sha256(path)}  {relative(path)}\n" for path in paths)


def render_repository_manifest(paths: list[Path]) -> str:
    files = [
        {
            "path": relative(path),
            "bytes": path.stat().st_size,
            "sha256": sha256(path),
        }
        for path in paths
    ]
    payload = {
        "name": "dennett",
        "edition": REPOSITORY_EDITION,
        "file_count": len(files),
        "files": files,
    }
    return json.dumps(payload, ensure_ascii=False, indent=2) + "\n"


def write_or_check(path: Path, content: str, check: bool) -> bool:
    if check:
        current = path.read_text(encoding="utf-8") if path.exists() else None
        if current != content:
            print(f"stale repository metadata: {relative(path)}")
            return False
        return True
    path.write_text(content, encoding="utf-8", newline="\n")
    return True


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()

    ok = write_or_check(DOCS_MANIFEST, render_docs_manifest(), args.check)
    if args.check and not ok:
        raise SystemExit(1)

    paths = repository_paths()
    docs_paths = [path for path in paths if path.is_relative_to(ROOT / "docs")]
    docs_paths = [path for path in docs_paths if path != DOCS_CHECKSUMS]
    ok = write_or_check(DOCS_CHECKSUMS, render_checksums(docs_paths), args.check) and ok
    if args.check and not ok:
        raise SystemExit(1)

    paths = repository_paths()
    manifest_paths = [
        path for path in paths if path not in {ROOT_MANIFEST, ROOT_CHECKSUMS}
    ]
    ok = write_or_check(
        ROOT_MANIFEST,
        render_repository_manifest(manifest_paths),
        args.check,
    ) and ok
    if args.check and not ok:
        raise SystemExit(1)

    paths = repository_paths()
    checksum_paths = [path for path in paths if path != ROOT_CHECKSUMS]
    ok = write_or_check(
        ROOT_CHECKSUMS,
        render_checksums(checksum_paths),
        args.check,
    ) and ok
    raise SystemExit(0 if ok else 1)


if __name__ == "__main__":
    main()
