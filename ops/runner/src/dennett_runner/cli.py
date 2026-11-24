"""Unified CLI verification runner (`dennett-check`) for Dennett milestone D0+."""

from __future__ import annotations

import argparse
from collections import deque
import json
import os
import re
import secrets
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import uuid
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Sequence
from urllib.parse import parse_qs, unquote, urlsplit
from urllib.request import ProxyHandler, Request, build_opener

MIN_AGY_VERSION = (1, 5, 0)
DEFAULT_CMD_TIMEOUT_SECONDS = 300
MIN_INTEGRATION_TESTS = 55
MIN_DESKTOP_FRONTEND_TESTS = 45

FAST_NEXTEST_FILTER = "not (binary(~integration) | binary(acp_live_probe) | binary(head_live_agy_e2e))"
INTEGRATION_NEXTEST_FILTER = "binary(~integration)"

REQUIRED_DOCS = (
    "AGENTS.md",
    "README.md",
    "docs/implementation/STATUS.md",
    "docs/implementation/04_MILESTONE_DEPENDENCY_MAP.md",
    "docs/implementation/01_AGENT_EXECUTION_PROTOCOL.md",
    "docs/testing/TEST_CATALOGUE_AND_QUALITY_GATES.md",
    "docs/testing/REQUIREMENTS_COVERAGE.md",
    "docs/testing/ANTIGRAVITY_RUNTIME_ACCEPTANCE.md",
    "docs/specifications/contracts/M_Runtime_Rollout_Contract.md",
    "ops/test-env/README.md",
)

FORBIDDEN_DEFERRED_RUNTIME_CRATES = (
    "codex",
    "openai",
    "async-openai",
    "anthropic",
    "claude",
)

FORBIDDEN_CONTRACTS_DEPS = (
    "postgres",
    "tokio-postgres",
    "sqlx",
    "rusqlite",
    "diesel",
    "hyper",
    "reqwest",
    "axum",
    "actix",
    "warp",
    "tauri",
    "wry",
    "egui",
    "tokio",
) + FORBIDDEN_DEFERRED_RUNTIME_CRATES

FORBIDDEN_MEMORYD_DEPS = (
    "dennett-testkit",
    "dennett-head",
    "dennett-ui",
    "dennett-desktop",
    "tauri",
    "wry",
    "egui",
) + FORBIDDEN_DEFERRED_RUNTIME_CRATES

FORBIDDEN_MEMORY_CLIENT_DEPS = (
    "dennett-memory-core",
    "dennett-memoryd",
    "dennett-testkit",
    "dennett-head",
    "dennett-ui",
    "dennett-desktop",
    "tokio-postgres",
    "deadpool-postgres",
    "postgres",
    "sqlx",
    "rusqlite",
    "diesel",
    "tauri",
    "wry",
    "egui",
) + FORBIDDEN_DEFERRED_RUNTIME_CRATES

FORBIDDEN_HEAD_DEPS = (
    "dennett-memory-core",
    "dennett-memoryd",
    "dennett-testkit",
    "tauri",
    "wry",
    "egui",
) + FORBIDDEN_DEFERRED_RUNTIME_CRATES

FORBIDDEN_HEAD_CLIENT_DEPS = (
    "dennett-head",
    "dennett-memory-core",
    "dennett-memoryd",
    "dennett-memory-client",
    "dennett-memory-mcp",
    "dennett-runtime-antigravity",
    "dennett-runtime-local",
    "dennett-testkit",
    "tokio-postgres",
    "deadpool-postgres",
    "postgres",
    "sqlx",
    "rusqlite",
    "diesel",
    "tauri",
    "wry",
    "egui",
) + FORBIDDEN_DEFERRED_RUNTIME_CRATES

FORBIDDEN_RUNTIME_ANTIGRAVITY_DEPS = (
    "dennett-head",
    "dennett-memory-core",
    "dennett-memoryd",
    "dennett-testkit",
    "tokio-postgres",
    "deadpool-postgres",
    "postgres",
    "sqlx",
    "rusqlite",
    "diesel",
    "tauri",
    "wry",
    "egui",
) + FORBIDDEN_DEFERRED_RUNTIME_CRATES

FORBIDDEN_DESKTOP_EXACT_DEPS = (
    "dennett-head",
    "dennett-memory-core",
    "dennett-memoryd",
    "dennett-memory-client",
    "dennett-memory-mcp",
    "dennett-control-mcp",
    "dennett-runtime-antigravity",
    "dennett-runtime-local",
    "dennett-testkit",
)

FORBIDDEN_DESKTOP_SUBSTRING_DEPS = (
    "tokio-postgres",
    "deadpool-postgres",
    "postgres",
    "sqlx",
    "rusqlite",
    "diesel",
) + FORBIDDEN_DEFERRED_RUNTIME_CRATES

ALLOWED_CONTRACTS_DIRECT_DEPS = {"serde", "thiserror", "uuid"}

MARKDOWN_LINK_RE = re.compile(r"!?\[[^\]]*\]\(([^)]+)\)")
NEXTEST_SUMMARY_RE = re.compile(
    r"Summary\s+\[[^\]]+\]\s+(?P<run>\d+)(?:/\d+)?\s+tests?\s+run:\s+(?P<rest>.*)"
)
COUNT_RE = re.compile(r"(?P<num>\d+)\s+(?P<label>passed|failed|skipped|leaky|timed out)")
SEMVER_RE = re.compile(r"(?<![\d.])v?(?P<major>\d+)\.(?P<minor>\d+)\.(?P<patch>\d+)\b")
LIBPQ_DBNAME_RE = re.compile(
    r"(?i)(?:^|\s)dbname\s*=\s*(?:'((?:\\.|''|[^'])*)'|([^\s]+))"
)


class CheckError(RuntimeError):
    """Raised when a verification gate fails."""


@dataclass(frozen=True)
class NextestSummary:
    """Parsed summary counters from a `cargo nextest run` invocation."""

    tests_run: int
    passed: int
    failed: int
    skipped: int
    leaky: int


def find_repo_root() -> Path:
    """Locate the repository root containing `Cargo.toml` and `AGENTS.md`."""
    candidates = [
        Path.cwd().resolve(),
        Path(__file__).resolve().parents[4],
    ]
    for base in candidates:
        for current in (base, *base.parents):
            if (current / "AGENTS.md").is_file() and (current / "Cargo.toml").is_file():
                return current
    raise CheckError(
        "Could not locate Dennett repository root (expected AGENTS.md and Cargo.toml)."
    )


def build_tool_env(
    overrides: dict[str, str] | None = None,
    removals: Sequence[str] = (),
) -> dict[str, str]:
    """Create a subprocess environment ensuring `~/.cargo/bin` is discoverable if needed."""
    env = os.environ.copy()
    for key in removals:
        env.pop(key, None)
    if overrides:
        env.update(overrides)

    if shutil.which("cargo", path=env.get("PATH")) is None:
        cargo_bin = Path.home() / ".cargo" / "bin"
        if cargo_bin.is_dir():
            env["PATH"] = f"{cargo_bin}{os.pathsep}{env.get('PATH', '')}"
    return env


def _safe_write_stream(stream: object, text: str) -> None:
    """Write UTF-8 text to stdout/stderr without crashing on legacy Windows cp1252 consoles."""
    if not text:
        return
    write = getattr(stream, "write", None)
    if write is None:
        return
    try:
        write(text)
    except UnicodeEncodeError:
        encoding = getattr(stream, "encoding", None) or "ascii"
        safe_text = text.encode(encoding, errors="replace").decode(encoding, errors="replace")
        write(safe_text)


def _configure_stdio_utf8() -> None:
    """Prefer UTF-8 with replacement on stdout/stderr so CI logs on Windows never fail on Unicode."""
    for stream in (sys.stdout, sys.stderr):
        reconfigure = getattr(stream, "reconfigure", None)
        if callable(reconfigure):
            try:
                reconfigure(encoding="utf-8", errors="replace")
            except (OSError, ValueError):
                pass


def run_cmd(
    args: Sequence[str],
    *,
    cwd: Path,
    env: dict[str, str] | None = None,
    capture: bool = False,
    echo_output: bool = True,
    check: bool = True,
    timeout_seconds: int = DEFAULT_CMD_TIMEOUT_SECONDS,
) -> subprocess.CompletedProcess[str]:
    """Execute a subprocess with UTF-8 decoding, timeout enforcement, and clear diagnostics."""
    effective_env = env if env is not None else build_tool_env()
    cmd_args = list(args)
    resolved_exe = shutil.which(cmd_args[0], path=effective_env.get("PATH"))
    if resolved_exe is not None:
        cmd_args[0] = resolved_exe

    display = " ".join(args)
    print(f"[dennett-check] $ {display}", flush=True)
    try:
        proc = subprocess.run(
            cmd_args,
            cwd=cwd,
            env=effective_env,
            text=True,
            encoding="utf-8",
            errors="replace",
            capture_output=capture,
            timeout=timeout_seconds,
            check=False,
        )
    except FileNotFoundError as exc:
        raise CheckError(f"Executable `{args[0]}` not found on PATH: {exc}") from exc
    except subprocess.TimeoutExpired as exc:
        raise CheckError(
            f"Command timed out after {timeout_seconds}s: {display}"
        ) from exc

    if capture and echo_output:
        if proc.stdout:
            _safe_write_stream(sys.stdout, proc.stdout)
        if proc.stderr:
            _safe_write_stream(sys.stderr, proc.stderr)
    if check and proc.returncode != 0:
        if capture and not echo_output:
            if proc.stdout:
                _safe_write_stream(sys.stdout, proc.stdout)
            if proc.stderr:
                _safe_write_stream(sys.stderr, proc.stderr)
        raise CheckError(f"Command failed with exit code {proc.returncode}: {display}")
    return proc


def parse_nextest_summary(output: str) -> NextestSummary:
    """Parse the final `Summary [...]` line from `cargo nextest` output."""
    matches = list(NEXTEST_SUMMARY_RE.finditer(output))
    if not matches:
        raise CheckError(
            "Could not find `Summary [...]` line in cargo-nextest output; refusing to mark green."
        )
    last = matches[-1]
    tests_run = int(last.group("run"))
    counts = {"passed": 0, "failed": 0, "skipped": 0, "leaky": 0}
    for item in COUNT_RE.finditer(last.group("rest")):
        label = item.group("label")
        num = int(item.group("num"))
        if label in counts:
            counts[label] += num
        elif label == "timed out":
            counts["failed"] += num

    return NextestSummary(
        tests_run=tests_run,
        passed=counts["passed"],
        failed=counts["failed"],
        skipped=counts["skipped"],
        leaky=counts["leaky"],
    )


def run_nextest_checked(
    nextest_args: Sequence[str],
    *,
    cwd: Path,
    env: dict[str, str],
    min_passed: int = 1,
    allow_skipped: bool = False,
    timeout_seconds: int = DEFAULT_CMD_TIMEOUT_SECONDS,
) -> tuple[NextestSummary, str]:
    """Run `cargo nextest run` and enforce non-empty, zero-failure, zero-leaky, zero-skip invariants."""
    proc = run_cmd(
        ["cargo", "nextest", "run", "--color", "never", *nextest_args],
        cwd=cwd,
        env=env,
        capture=True,
        check=False,
        timeout_seconds=timeout_seconds,
    )
    combined = f"{proc.stdout}\n{proc.stderr}"
    summary = parse_nextest_summary(combined)

    if proc.returncode != 0:
        raise CheckError(
            f"cargo-nextest exited with code {proc.returncode} "
            f"(run={summary.tests_run}, passed={summary.passed}, "
            f"failed={summary.failed}, skipped={summary.skipped}, leaky={summary.leaky})."
        )
    if summary.failed > 0:
        raise CheckError(f"cargo-nextest reported {summary.failed} failed test(s).")
    if summary.leaky > 0:
        raise CheckError(
            f"cargo-nextest reported {summary.leaky} leaky test(s); leaked subprocess handles are forbidden."
        )
    if summary.passed < min_passed:
        raise CheckError(
            f"cargo-nextest passed {summary.passed} test(s), below required minimum {min_passed} "
            f"(tests_run={summary.tests_run}). Empty test runs cannot pass."
        )
    if not allow_skipped and summary.skipped > 0:
        raise CheckError(
            f"cargo-nextest skipped {summary.skipped} test(s); skipped tests are forbidden here."
        )

    print(
        f"[dennett-check] nextest verified: run={summary.tests_run}, "
        f"passed={summary.passed}, failed={summary.failed}, "
        f"skipped={summary.skipped}, leaky={summary.leaky}",
        flush=True,
    )
    return summary, combined


def strip_markdown_code(text: str) -> str:
    """Remove fenced code blocks and inline code spans before scanning markdown links."""
    lines: list[str] = []
    in_fence = False
    fence_marker = ""
    for line in text.splitlines():
        stripped = line.lstrip()
        if not in_fence:
            if stripped.startswith("```") or stripped.startswith("~~~"):
                in_fence = True
                fence_marker = stripped[:3]
                continue
            lines.append(re.sub(r"`[^`]*`", "", line))
        else:
            if stripped.startswith(fence_marker):
                in_fence = False
                fence_marker = ""
    return "\n".join(lines)


def collect_markdown_files(repo_root: Path) -> list[Path]:
    """Collect root `.md`, `docs/**/*.md` (excluding `docs/archive/`), and `ops/test-env/README.md`."""
    files: list[Path] = []
    for path in sorted(repo_root.glob("*.md")):
        if path.is_file():
            files.append(path)

    docs_dir = repo_root / "docs"
    archive_dir = (docs_dir / "archive").resolve()
    if docs_dir.is_dir():
        for path in sorted(docs_dir.rglob("*.md")):
            resolved = path.resolve()
            if archive_dir in resolved.parents or resolved == archive_dir:
                continue
            files.append(path)

    test_env_readme = repo_root / "ops" / "test-env" / "README.md"
    if test_env_readme.is_file():
        files.append(test_env_readme)

    return files


def check_docs(repo_root: Path) -> None:
    """Validate required documentation, local markdown links, and Antigravity contract references."""
    missing = [rel for rel in REQUIRED_DOCS if not (repo_root / rel).is_file()]
    if missing:
        raise CheckError(f"Missing required documentation files: {', '.join(missing)}")

    md_files = collect_markdown_files(repo_root)
    if not md_files:
        raise CheckError("No markdown files discovered for docs verification.")

    broken_links: list[str] = []
    checked_links = 0
    resolved_root = repo_root.resolve()

    for md_file in md_files:
        raw = md_file.read_text(encoding="utf-8")
        cleaned = strip_markdown_code(raw)
        for match in MARKDOWN_LINK_RE.finditer(cleaned):
            raw_target = match.group(1).strip()
            if not raw_target:
                continue
            if raw_target.startswith("<") and ">" in raw_target:
                raw_target = raw_target[1 : raw_target.index(">")]
            else:
                raw_target = raw_target.split()[0]

            lower = raw_target.lower()
            if lower.startswith(
                (
                    "http://",
                    "https://",
                    "mailto:",
                    "tel:",
                    "data:",
                    "file://",
                    "conversation://",
                    "#",
                )
            ):
                continue

            path_part = raw_target.split("#", 1)[0].split("?", 1)[0]
            path_part = unquote(path_part)
            if not path_part:
                continue

            checked_links += 1
            if path_part.startswith("/"):
                candidate = (resolved_root / path_part.lstrip("/")).resolve()
            else:
                candidate = (md_file.parent / path_part).resolve()

            try:
                candidate.relative_to(resolved_root)
            except ValueError:
                rel_src = md_file.relative_to(repo_root)
                broken_links.append(f"{rel_src} -> {raw_target} (escapes repository root)")
                continue

            if not candidate.exists():
                rel_src = md_file.relative_to(repo_root)
                broken_links.append(f"{rel_src} -> {raw_target} (target does not exist)")

    if checked_links == 0:
        raise CheckError("No local markdown links were verified; link checker produced empty set.")
    if broken_links:
        details = "\n  - ".join(broken_links)
        raise CheckError(f"Found {len(broken_links)} broken local markdown link(s):\n  - {details}")

    for rel in (
        "AGENTS.md",
        "docs/specifications/contracts/M_Runtime_Rollout_Contract.md",
    ):
        text = (repo_root / rel).read_text(encoding="utf-8")
        if "agy acp" not in text:
            raise CheckError(f"{rel} must reference `agy acp` as the primary runtime path.")
        if "ANTIGRAVITY_RUNTIME_ACCEPTANCE.md" not in text:
            raise CheckError(f"{rel} must reference `ANTIGRAVITY_RUNTIME_ACCEPTANCE.md`.")

    print(
        f"[dennett-check] docs OK: {len(md_files)} markdown files checked, "
        f"{checked_links} local links valid.",
        flush=True,
    )


def check_deps(repo_root: Path) -> None:
    """Verify pinned toolchain, Cargo.lock/pnpm-lock.yaml presence, and crate architectural boundaries."""
    for required in (
        "rust-toolchain.toml",
        "Cargo.toml",
        "Cargo.lock",
        "apps/desktop/package.json",
        "apps/desktop/pnpm-lock.yaml",
    ):
        if not (repo_root / required).is_file():
            raise CheckError(f"Missing required build/pin file: {required}")

    proc = run_cmd(
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        cwd=repo_root,
        capture=True,
        echo_output=False,
        check=True,
    )
    meta = json.loads(proc.stdout)

    packages_by_id = {pkg["id"]: pkg for pkg in meta["packages"]}
    packages_by_name = {pkg["name"]: pkg for pkg in meta["packages"]}
    resolve_nodes = {node["id"]: node for node in meta["resolve"]["nodes"]}

    def transitive_prod_dep_names(root_pkg_name: str) -> set[str]:
        """Walk normal and build dependencies only (excluding dev-only edges)."""
        if root_pkg_name not in packages_by_name:
            raise CheckError(f"Expected workspace package `{root_pkg_name}` in cargo metadata.")
        root_id = packages_by_name[root_pkg_name]["id"]
        visited: set[str] = set()
        stack = [root_id]
        names: set[str] = set()
        while stack:
            current_id = stack.pop()
            if current_id in visited:
                continue
            visited.add(current_id)
            node = resolve_nodes.get(current_id)
            if not node:
                continue
            for dep in node.get("deps", []):
                dep_kinds = dep.get("dep_kinds", [])
                is_prod_edge = not dep_kinds or any(
                    dk.get("kind") in (None, "build") for dk in dep_kinds
                )
                if not is_prod_edge:
                    continue
                dep_id = dep["pkg"]
                dep_name = packages_by_id[dep_id]["name"]
                names.add(dep_name)
                stack.append(dep_id)
        return names

    # Global deferred-provider ban spans the entire Cargo.lock (including dev-dependencies).
    all_crate_names = {pkg["name"].lower() for pkg in meta["packages"]}
    for forbidden in FORBIDDEN_DEFERRED_RUNTIME_CRATES:
        hits = [name for name in all_crate_names if forbidden in name]
        if hits:
            raise CheckError(
                f"Deferred provider crate(s) detected in Cargo graph ({forbidden}): {', '.join(sorted(hits))}"
            )

    contracts_pkg = packages_by_name.get("dennett-contracts")
    if contracts_pkg is None:
        raise CheckError("Workspace is missing `dennett-contracts`.")
    direct_contracts = {
        dep["name"]
        for dep in contracts_pkg["dependencies"]
        if dep.get("kind") in (None, "normal")
    }
    unexpected_direct = direct_contracts - ALLOWED_CONTRACTS_DIRECT_DEPS
    if unexpected_direct:
        raise CheckError(
            f"`dennett-contracts` has unexpected direct dependencies: {', '.join(sorted(unexpected_direct))}"
        )
    contracts_trans = {n.lower() for n in transitive_prod_dep_names("dennett-contracts")}
    for forbidden in FORBIDDEN_CONTRACTS_DEPS:
        hits = [name for name in contracts_trans if name == forbidden or forbidden in name]
        if hits:
            raise CheckError(
                f"`dennett-contracts` violates boundary by depending on: {', '.join(sorted(hits))}"
            )

    for pkg_name in ("dennett-memoryd", "dennett-memory-core"):
        trans = {n.lower() for n in transitive_prod_dep_names(pkg_name)}
        for forbidden in FORBIDDEN_MEMORYD_DEPS:
            hits = [name for name in trans if name == forbidden or forbidden in name]
            if hits:
                raise CheckError(
                    f"`{pkg_name}` violates standalone boundary by depending on: {', '.join(sorted(hits))}"
                )

    for client_pkg in ("dennett-memory-client", "dennett-memory-mcp"):
        trans = {n.lower() for n in transitive_prod_dep_names(client_pkg)}
        for forbidden in FORBIDDEN_MEMORY_CLIENT_DEPS:
            hits = [name for name in trans if name == forbidden or forbidden in name]
            if hits:
                raise CheckError(
                    f"`{client_pkg}` violates thin-client boundary by depending on: {', '.join(sorted(hits))}"
                )

    head_trans = {n.lower() for n in transitive_prod_dep_names("dennett-head")}
    for forbidden in FORBIDDEN_HEAD_DEPS:
        hits = [name for name in head_trans if name == forbidden or forbidden in name]
        if hits:
            raise CheckError(
                f"`dennett-head` violates Memory separation boundary by depending on: {', '.join(sorted(hits))}"
            )

    for head_thin_pkg in ("dennett-head-client", "dennett-control-mcp"):
        trans = {n.lower() for n in transitive_prod_dep_names(head_thin_pkg)}
        for forbidden in FORBIDDEN_HEAD_CLIENT_DEPS:
            if forbidden.startswith("dennett-"):
                hits = [name for name in trans if name == forbidden]
            else:
                hits = [name for name in trans if name == forbidden or forbidden in name]
            if hits:
                raise CheckError(
                    f"`{head_thin_pkg}` violates thin-client boundary by depending on: {', '.join(sorted(hits))}"
                )

    for runtime_pkg in ("dennett-runtime-antigravity", "dennett-runtime-local"):
        runtime_trans = {n.lower() for n in transitive_prod_dep_names(runtime_pkg)}
        for forbidden in FORBIDDEN_RUNTIME_ANTIGRAVITY_DEPS:
            hits = [name for name in runtime_trans if name == forbidden or forbidden in name]
            if hits:
                raise CheckError(
                    f"`{runtime_pkg}` violates adapter boundary by depending on: {', '.join(sorted(hits))}"
                )

    desktop_trans = {n.lower() for n in transitive_prod_dep_names("dennett-desktop")}
    exact_hits = sorted(name for name in desktop_trans if name in FORBIDDEN_DESKTOP_EXACT_DEPS)
    if exact_hits:
        raise CheckError(
            f"`dennett-desktop` violates thin UI client boundary by depending on: {', '.join(exact_hits)}"
        )
    for forbidden in FORBIDDEN_DESKTOP_SUBSTRING_DEPS:
        hits = [name for name in desktop_trans if name == forbidden or forbidden in name]
        if hits:
            raise CheckError(
                f"`dennett-desktop` violates thin UI client boundary by depending on: {', '.join(sorted(hits))}"
            )

    print(
        f"[dennett-check] deps OK: {len(meta['packages'])} locked crates verified, "
        f"boundaries intact (contracts direct={sorted(direct_contracts)}, desktop isolated).",
        flush=True,
    )


def run_desktop_frontend_checks(repo_root: Path, env: dict[str, str]) -> None:
    """Run TypeScript typecheck, Vitest suite (zero-skip, non-empty), and Vite production build."""
    desktop_dir = repo_root / "apps" / "desktop"
    if not (desktop_dir / "package.json").is_file():
        raise CheckError(f"Missing desktop frontend package.json at {desktop_dir}")
    if not (desktop_dir / "node_modules").is_dir():
        raise CheckError(
            "Desktop frontend dependencies are not installed; run "
            "`pnpm --dir apps/desktop install --frozen-lockfile` first."
        )

    run_cmd(
        ["pnpm", "--dir", "apps/desktop", "run", "typecheck"],
        cwd=repo_root,
        env=env,
    )

    with tempfile.TemporaryDirectory(prefix="dennett-vitest-") as tmp_dir:
        report_path = Path(tmp_dir) / "vitest-report.json"
        proc = run_cmd(
            [
                "pnpm",
                "--dir",
                "apps/desktop",
                "exec",
                "vitest",
                "run",
                "--reporter=default",
                "--reporter=json",
                f"--outputFile={report_path}",
            ],
            cwd=repo_root,
            env=env,
            capture=True,
            check=False,
        )
        if proc.returncode != 0:
            raise CheckError(
                f"Desktop Vitest suite exited with code {proc.returncode}."
            )
        if not report_path.is_file():
            raise CheckError(
                "Vitest did not emit its JSON report; refusing to mark desktop tests green."
            )
        report = json.loads(report_path.read_text(encoding="utf-8"))
        total = int(report.get("numTotalTests", 0))
        passed = int(report.get("numPassedTests", 0))
        failed = int(report.get("numFailedTests", 0))
        skipped = int(report.get("numPendingTests", 0)) + int(report.get("numTodoTests", 0))
        success = bool(report.get("success", False))

        if not success or failed > 0:
            raise CheckError(
                f"Desktop Vitest reported failure (total={total}, passed={passed}, failed={failed})."
            )
        if skipped > 0:
            raise CheckError(
                f"Desktop Vitest skipped {skipped} test(s); skipped tests are forbidden."
            )
        if passed < MIN_DESKTOP_FRONTEND_TESTS:
            raise CheckError(
                f"Desktop Vitest passed {passed} test(s), below required minimum "
                f"{MIN_DESKTOP_FRONTEND_TESTS}. Empty test runs cannot pass."
            )
        print(
            f"[dennett-check] desktop Vitest verified: total={total}, passed={passed}, "
            f"failed={failed}, skipped={skipped}",
            flush=True,
        )

    run_cmd(
        ["pnpm", "--dir", "apps/desktop", "run", "build"],
        cwd=repo_root,
        env=env,
    )


def check_fast(repo_root: Path) -> None:
    """Run desktop frontend checks plus rustfmt, strict clippy, rustdoc, doctests, and fast unit/safety tests."""
    clean_env = build_tool_env(
        removals=("DENNETT_TEST_PROFILE", "DENNETT_FORCE_INTENTIONAL_FAILURE")
    )
    doc_env = build_tool_env(
        overrides={"RUSTDOCFLAGS": "-D warnings"},
        removals=("DENNETT_TEST_PROFILE", "DENNETT_FORCE_INTENTIONAL_FAILURE"),
    )
    run_desktop_frontend_checks(repo_root, clean_env)
    run_cmd(["cargo", "fmt", "--all", "--", "--check"], cwd=repo_root, env=clean_env)
    run_cmd(
        ["cargo", "clippy", "--workspace", "--all-targets", "--", "-D", "warnings"],
        cwd=repo_root,
        env=clean_env,
    )
    run_cmd(["cargo", "doc", "--workspace", "--no-deps"], cwd=repo_root, env=doc_env)
    run_cmd(["cargo", "test", "--doc", "--workspace"], cwd=repo_root, env=clean_env)
    run_nextest_checked(
        ["--workspace", "--all-targets", "-E", FAST_NEXTEST_FILTER],
        cwd=repo_root,
        env=clean_env,
        min_passed=1,
        allow_skipped=False,
    )
    print("[dennett-check] fast OK.", flush=True)


def check_self_check(repo_root: Path) -> None:
    """Prove the test runner rejects 0-test runs, propagates intentional failures, enforces guards, and blocks DSN bypasses."""
    clean_env = build_tool_env(
        removals=("DENNETT_TEST_PROFILE", "DENNETT_FORCE_INTENTIONAL_FAILURE")
    )

    # 1. Prove a 0-test filter run is rejected by OUR parser even if nextest exits 0 (--no-tests=pass).
    print("[dennett-check] self-check (1/4): verifying 0-test run is rejected...", flush=True)
    zero_rejected = False
    try:
        run_nextest_checked(
            [
                "-p",
                "dennett-testkit",
                "--test",
                "unit_safety",
                "--no-tests=pass",
                "dennett_nonexistent_test_xyz",
            ],
            cwd=repo_root,
            env=clean_env,
            min_passed=1,
            allow_skipped=True,
        )
    except CheckError as exc:
        if "Empty test runs cannot pass" in str(exc):
            zero_rejected = True
            print(f"[dennett-check] self-check (1/4) confirmed rejection: {exc}", flush=True)
        else:
            raise CheckError(
                f"Self-check (1/4) failed for unexpected reason instead of empty-run guard: {exc}"
            ) from exc
    if not zero_rejected:
        raise CheckError("Self-check FAILED: runner allowed a 0-test nextest invocation to pass!")

    # 2. Prove an intentional test failure fails the overall runner gate.
    print(
        "[dennett-check] self-check (2/4): verifying intentional failure canary is caught...",
        flush=True,
    )
    canary_env = build_tool_env(
        overrides={"DENNETT_FORCE_INTENTIONAL_FAILURE": "1"},
        removals=("DENNETT_TEST_PROFILE",),
    )
    canary_proc = run_cmd(
        [
            "cargo",
            "nextest",
            "run",
            "--color",
            "never",
            "-p",
            "dennett-testkit",
            "--test",
            "unit_safety",
            "intentional_failure_canary",
        ],
        cwd=repo_root,
        env=canary_env,
        capture=True,
        check=False,
    )
    combined_canary = f"{canary_proc.stdout}\n{canary_proc.stderr}"
    if canary_proc.returncode == 0:
        raise CheckError(
            "Self-check FAILED: intentional_failure_canary returned exit code 0 when forced to fail!"
        )
    if "INTENTIONAL_FAILURE_CANARY_TRIGGERED" not in combined_canary:
        raise CheckError(
            "Self-check FAILED: intentional_failure_canary did not emit expected failure marker."
        )
    summary = parse_nextest_summary(combined_canary)
    if summary.failed < 1:
        raise CheckError(
            f"Self-check FAILED: expected failed>=1 for intentional canary, got {summary}."
        )
    print(
        f"[dennett-check] self-check (2/4) confirmed intentional failure propagation "
        f"(exit={canary_proc.returncode}, failed={summary.failed}).",
        flush=True,
    )

    # 3. Prove FaultInjectionGuard fails closed when DENNETT_TEST_PROFILE is absent.
    # Filtering to a single test marks the remaining tests in `unit_safety` as filtered/skipped by nextest.
    print(
        "[dennett-check] self-check (3/4): verifying FaultInjectionGuard rejects missing DENNETT_TEST_PROFILE...",
        flush=True,
    )
    guard_summary, _ = run_nextest_checked(
        [
            "-p",
            "dennett-testkit",
            "--test",
            "unit_safety",
            "fault_injection_guard_fails_closed_without_env_marker",
        ],
        cwd=repo_root,
        env=clean_env,
        min_passed=1,
        allow_skipped=True,
    )
    if guard_summary.tests_run != 1 or guard_summary.passed != 1:
        raise CheckError(
            f"Self-check (3/4) expected exactly 1 guard test to run and pass, got {guard_summary}."
        )

    # 4. Prove validate_test_pg_url rejects H0-02 DSN override exploits and non-_test DBs.
    print(
        "[dennett-check] self-check (4/4): verifying DSN `*_test` validator blocks override exploits...",
        flush=True,
    )
    for bad_dsn in (
        "postgres://u:p@127.0.0.1:55432/fake_test?dbname=production_db",
        "postgres://u:p@127.0.0.1:55432/dennett_memory_test?dbname=a_test&dbname=b_test",
        "host=127.0.0.1 user=u password=p dbname=fake_test dbname=production_db",
        "postgres://u:p@127.0.0.1:55432/production_db",
    ):
        try:
            validate_test_pg_url("SELF_CHECK_PG_URL", bad_dsn)
        except CheckError:
            continue
        raise CheckError(f"Self-check (4/4) FAILED: validate_test_pg_url accepted `{bad_dsn}`!")
    validate_test_pg_url(
        "SELF_CHECK_PG_URL",
        "postgres://u:p@127.0.0.1:55432/dennett_memory_test?sslmode=disable",
    )
    validate_test_pg_url(
        "SELF_CHECK_PG_URL",
        "host=127.0.0.1 user=u password=p dbname=dennett_memory_test",
    )

    print("[dennett-check] self-check OK (all 4 harness invariants verified).", flush=True)


def validate_test_pg_url(var_name: str, pg_url: str) -> None:
    """Validate that a PostgreSQL URI or libpq DSN unambiguously targets a `*_test` database."""
    trimmed = pg_url.strip()
    if not trimmed:
        raise CheckError(f"{var_name} must not be empty.")

    if "://" in trimmed:
        parsed = urlsplit(trimmed)
        if parsed.scheme.lower() not in ("postgres", "postgresql"):
            raise CheckError(
                f"{var_name} must use postgres:// or postgresql:// scheme, got {parsed.scheme!r}."
            )
        path_db = unquote(parsed.path.lstrip("/")).strip()
        qs = parse_qs(parsed.query, keep_blank_values=True)
        query_dbs = [
            unquote(val).strip()
            for key, vals in qs.items()
            if key.lower() == "dbname"
            for val in vals
        ]
        if len(query_dbs) > 1:
            raise CheckError(
                f"{var_name} contains conflicting `dbname` query parameters; refusing ambiguous DSN."
            )
        if path_db and query_dbs and path_db != query_dbs[0]:
            raise CheckError(
                f"{var_name} has conflicting path database ({path_db!r}) and query dbname ({query_dbs[0]!r})."
            )
        db_name = query_dbs[0] if query_dbs else path_db
    else:
        matches = list(LIBPQ_DBNAME_RE.finditer(trimmed))
        if not matches:
            raise CheckError(f"{var_name} is missing a `dbname=...` entry.")
        if len(matches) > 1:
            raise CheckError(
                f"{var_name} contains multiple `dbname=...` entries; refusing ambiguous libpq DSN."
            )
        raw_db = matches[0].group(1) if matches[0].group(1) is not None else matches[0].group(2)
        db_name = (raw_db or "").replace("\\'", "'").replace("''", "'").strip()

    if not db_name or len(db_name) <= len("_test") or not db_name.endswith("_test"):
        raise CheckError(
            f"{var_name} must target an isolated `*_test` PostgreSQL database (parsed db={db_name!r})."
        )


def resolve_compose_host_port(
    repo_root: Path, compose_file: Path, project_name: str
) -> int:
    """Inspect the actual ephemeral loopback port assigned to `postgres-test:5432`."""
    proc = run_cmd(
        [
            "docker",
            "compose",
            "-p",
            project_name,
            "-f",
            str(compose_file),
            "port",
            "postgres-test",
            "5432",
        ],
        cwd=repo_root,
        capture=True,
        echo_output=False,
        check=True,
        timeout_seconds=30,
    )
    for line in proc.stdout.splitlines():
        stripped = line.strip()
        if not stripped or ":" not in stripped:
            continue
        port_str = stripped.rsplit(":", 1)[-1]
        if port_str.isdigit():
            port = int(port_str)
            if 1 <= port <= 65535:
                return port
    raise CheckError(
        f"Could not parse host port from `docker compose port postgres-test 5432`: {proc.stdout!r}"
    )


def run_integration_suites(repo_root: Path, pg_env: dict[str, str]) -> None:
    """Execute all `*integration*` binaries discovered dynamically via cargo-nextest filterset."""
    run_nextest_checked(
        ["--workspace", "--all-targets", "-E", INTEGRATION_NEXTEST_FILTER],
        cwd=repo_root,
        env=pg_env,
        min_passed=MIN_INTEGRATION_TESTS,
        allow_skipped=False,
    )


def check_integration(repo_root: Path, *, manage_pg: bool) -> None:
    """Run real PostgreSQL isolation and standalone `dennett-memoryd` integration tests."""
    compose_file = repo_root / "ops" / "test-env" / "docker-compose.test.yml"

    run_cmd(
        [
            "cargo",
            "build",
            "--bin",
            "dennett-memoryd",
            "--bin",
            "dennett-memory-cli",
            "--bin",
            "dennett-memory-mcp",
            "--bin",
            "dennett-control-mcp",
            "--bin",
            "dennett-head",
            "--bin",
            "dennettctl",
            "--bin",
            "dennett-d0-mcp-fixture",
        ],
        cwd=repo_root,
    )

    scrub_prod_pg = ("DENNETT_MEMORY_PG_URL", "DENNETT_CONTROL_PG_URL")

    if manage_pg:
        if not compose_file.is_file():
            raise CheckError(f"Missing docker compose file: {compose_file}")
        project_name = f"dennett-test-{uuid.uuid4().hex[:8]}"
        compose_base = ["docker", "compose", "-p", project_name, "-f", str(compose_file)]
        try:
            run_cmd([*compose_base, "up", "-d", "--wait"], cwd=repo_root)
            host_port = resolve_compose_host_port(repo_root, compose_file, project_name)
            mem_url = (
                f"postgres://dennett_memory_role:dennett_memory_test_pw"
                f"@127.0.0.1:{host_port}/dennett_memory_test"
            )
            ctrl_url = (
                f"postgres://dennett_control_role:dennett_control_test_pw"
                f"@127.0.0.1:{host_port}/dennett_control_test"
            )
            validate_test_pg_url("DENNETT_TEST_MEMORY_PG_URL", mem_url)
            validate_test_pg_url("DENNETT_TEST_CONTROL_PG_URL", ctrl_url)
            pg_env = build_tool_env(
                overrides={
                    "DENNETT_TEST_PROFILE": "1",
                    "DENNETT_TEST_MEMORY_PG_URL": mem_url,
                    "DENNETT_TEST_CONTROL_PG_URL": ctrl_url,
                },
                removals=scrub_prod_pg,
            )
            run_integration_suites(repo_root, pg_env)
        finally:
            run_cmd(
                [*compose_base, "down", "-v"],
                cwd=repo_root,
                check=False,
                timeout_seconds=60,
            )
    else:
        mem_url = os.environ.get("DENNETT_TEST_MEMORY_PG_URL", "").strip()
        ctrl_url = os.environ.get("DENNETT_TEST_CONTROL_PG_URL", "").strip()
        if not mem_url or not ctrl_url:
            raise CheckError(
                "Integration tests require DENNETT_TEST_MEMORY_PG_URL and "
                "DENNETT_TEST_CONTROL_PG_URL (or pass `--manage-pg` to launch the local "
                "Docker test profile). Refusing to silently skip."
            )
        validate_test_pg_url("DENNETT_TEST_MEMORY_PG_URL", mem_url)
        validate_test_pg_url("DENNETT_TEST_CONTROL_PG_URL", ctrl_url)
        pg_env = build_tool_env(
            overrides={"DENNETT_TEST_PROFILE": "1"},
            removals=scrub_prod_pg,
        )
        run_integration_suites(repo_root, pg_env)

    print("[dennett-check] integration OK.", flush=True)


def parse_semver_tuple(raw: str) -> tuple[int, int, int]:
    """Extract the first MAJOR.MINOR.PATCH tuple from version output."""
    match = SEMVER_RE.search(raw)
    if not match:
        raise CheckError(f"Could not parse semantic version from `agy --version` output: {raw!r}")
    return (
        int(match.group("major")),
        int(match.group("minor")),
        int(match.group("patch")),
    )


def count_real_agy_conversations() -> int:
    """Count entries in the user's real `~/.gemini/antigravity-cli/conversations` directory."""
    conv_dir = Path.home() / ".gemini" / "antigravity-cli" / "conversations"
    if not conv_dir.is_dir():
        return -1
    return sum(1 for _ in conv_dir.iterdir())


def check_runtime_probe(repo_root: Path) -> None:
    """Run live `agy acp` D0 + D1 E2E probes (A01–A06, A09, V04) with isolated PG and ~/.gemini check."""
    agy_bin = os.environ.get("DENNETT_AGY_BIN", "").strip() or "agy"
    if shutil.which(agy_bin) is None and not Path(agy_bin).is_file():
        raise CheckError(
            f"Antigravity CLI binary `{agy_bin}` not found on PATH; cannot run live runtime-probe."
        )

    ver_proc = run_cmd(
        [agy_bin, "--version"],
        cwd=repo_root,
        capture=True,
        echo_output=False,
        check=True,
        timeout_seconds=30,
    )
    detected = parse_semver_tuple(ver_proc.stdout or ver_proc.stderr)
    if detected < MIN_AGY_VERSION:
        raise CheckError(
            f"Detected agy version {detected[0]}.{detected[1]}.{detected[2]} is below "
            f"minimum required {MIN_AGY_VERSION[0]}.{MIN_AGY_VERSION[1]}.{MIN_AGY_VERSION[2]}."
        )
    print(
        f"[dennett-check] detected agy {detected[0]}.{detected[1]}.{detected[2]} "
        f"(>= {MIN_AGY_VERSION[0]}.{MIN_AGY_VERSION[1]}.{MIN_AGY_VERSION[2]}).",
        flush=True,
    )

    compose_file = repo_root / "ops" / "test-env" / "docker-compose.test.yml"
    if not compose_file.is_file():
        raise CheckError(f"Missing docker compose file: {compose_file}")

    run_cmd(
        [
            "cargo",
            "build",
            "--bin",
            "dennett-memoryd",
            "--bin",
            "dennett-memory-mcp",
            "--bin",
            "dennett-control-mcp",
            "--bin",
            "dennett-head",
            "--bin",
            "dennettctl",
            "--bin",
            "dennett-d0-mcp-fixture",
        ],
        cwd=repo_root,
    )

    conv_before = count_real_agy_conversations()
    scrub_prod_pg = ("DENNETT_MEMORY_PG_URL", "DENNETT_CONTROL_PG_URL")
    project_name = f"dennett-probe-{uuid.uuid4().hex[:8]}"
    compose_base = ["docker", "compose", "-p", project_name, "-f", str(compose_file)]

    try:
        run_cmd([*compose_base, "up", "-d", "--wait"], cwd=repo_root)
        host_port = resolve_compose_host_port(repo_root, compose_file, project_name)
        mem_url = (
            f"postgres://dennett_memory_role:dennett_memory_test_pw"
            f"@127.0.0.1:{host_port}/dennett_memory_test"
        )
        ctrl_url = (
            f"postgres://dennett_control_role:dennett_control_test_pw"
            f"@127.0.0.1:{host_port}/dennett_control_test"
        )
        validate_test_pg_url("DENNETT_TEST_MEMORY_PG_URL", mem_url)
        validate_test_pg_url("DENNETT_TEST_CONTROL_PG_URL", ctrl_url)

        probe_env = build_tool_env(
            overrides={
                "DENNETT_TEST_PROFILE": "1",
                "DENNETT_LIVE_ACP_PROBE": "1",
                "DENNETT_TEST_MEMORY_PG_URL": mem_url,
                "DENNETT_TEST_CONTROL_PG_URL": ctrl_url,
            },
            removals=scrub_prod_pg,
        )
        run_nextest_checked(
            [
                "-p",
                "dennett-testkit",
                "--test",
                "acp_live_probe",
                "--test",
                "head_live_agy_e2e",
                "--run-ignored",
                "only",
                "--test-threads",
                "1",
            ],
            cwd=repo_root,
            env=probe_env,
            min_passed=4,
            allow_skipped=False,
            timeout_seconds=420,
        )
    finally:
        run_cmd(
            [*compose_base, "down", "-v"],
            cwd=repo_root,
            check=False,
            timeout_seconds=60,
        )

    conv_after = count_real_agy_conversations()
    if conv_before != conv_after:
        raise CheckError(
            f"Live runtime-probe polluted real ~/.gemini/antigravity-cli/conversations "
            f"(before={conv_before}, after={conv_after})!"
        )
    print(
        f"[dennett-check] runtime-probe OK (real ~/.gemini conversations untouched: "
        f"before={conv_before} after={conv_after}).",
        flush=True,
    )


def _spawn_pipe_drainer(
    stream: Any,
    ring: deque[str],
    *,
    ready_event: threading.Event | None = None,
    addr_box: list[str] | None = None,
) -> None:
    """Continuously drain a child stdout/stderr pipe in a daemon thread so OS pipe buffers never deadlock."""
    if stream is None:
        return

    def _drain() -> None:
        try:
            for line in iter(stream.readline, ""):
                stripped = line.strip()
                if stripped:
                    ring.append(stripped)
                    if (
                        ready_event is not None
                        and addr_box is not None
                        and not ready_event.is_set()
                        and stripped.startswith("LISTENING ")
                    ):
                        addr_box.append(stripped.removeprefix("LISTENING ").strip())
                        ready_event.set()
        except Exception:
            pass
        finally:
            try:
                stream.close()
            except Exception:
                pass

    thread = threading.Thread(target=_drain, daemon=True)
    thread.start()


def _read_listening_addr(proc: subprocess.Popen[str], label: str, timeout_s: float = 20.0) -> str:
    """Read `LISTENING <ip:port>` using background pipe drainer threads with a real deadline (`D2-OPS-01`)."""
    if proc.stdout is None:
        raise CheckError(f"{label} stdout was not captured.")
    stdout_ring: deque[str] = deque(maxlen=200)
    stderr_ring: deque[str] = deque(maxlen=200)
    ready_event = threading.Event()
    addr_box: list[str] = []

    _spawn_pipe_drainer(proc.stdout, stdout_ring, ready_event=ready_event, addr_box=addr_box)
    _spawn_pipe_drainer(proc.stderr, stderr_ring)

    deadline = time.monotonic() + timeout_s
    while time.monotonic() < deadline:
        if ready_event.wait(timeout=0.1):
            if addr_box:
                return addr_box[0]
        if proc.poll() is not None:
            # Give drainer threads a brief moment to collect final stderr lines.
            time.sleep(0.05)
            stderr_tail = "\n".join(stderr_ring).strip()
            raise CheckError(
                f"{label} exited early with code {proc.returncode}: {stderr_tail}"
            )
    stderr_tail = "\n".join(stderr_ring).strip()
    raise CheckError(
        f"Timed out after {timeout_s:.1f}s waiting for LISTENING line from {label}"
        + (f" (stderr tail: {stderr_tail})" if stderr_tail else ".")
    )


def _graceful_stop_daemon(
    proc: subprocess.Popen[str] | None,
    label: str,
    *,
    base_url: str | None = None,
) -> None:
    """Stop a spawned demo daemon cleanly: HTTP `/shutdown` -> terminate -> Windows `taskkill /F /T` (`D2-OPS-01`)."""
    if proc is None or proc.poll() is not None:
        return

    if base_url:
        try:
            opener = build_opener(ProxyHandler({}))
            req = Request(
                f"{base_url.rstrip('/')}/shutdown",
                data=b"",
                headers={"X-Dennett-Shutdown": "1"},
                method="POST",
            )
            with opener.open(req, timeout=1.5):
                pass
            proc.wait(timeout=4)
            return
        except Exception:
            pass

    if proc.poll() is not None:
        return

    try:
        proc.terminate()
        proc.wait(timeout=3)
        return
    except Exception:
        pass

    if os.name == "nt":
        try:
            subprocess.run(
                ["taskkill", "/F", "/T", "/PID", str(proc.pid)],
                check=False,
                capture_output=True,
                timeout=5,
            )
            proc.wait(timeout=3)
            return
        except Exception:
            pass

    try:
        proc.kill()
        proc.wait(timeout=5)
    except Exception:
        print(f"[dennett-check] warning: could not verify exit of {label}", file=sys.stderr)


def _post_loopback_json(base_url: str, path: str, payload: dict[str, Any]) -> dict[str, Any]:
    """Send a loopback JSON POST request directly to `dennett-head` (bypassing proxies)."""
    opener = build_opener(ProxyHandler({}))
    body = json.dumps(payload).encode("utf-8")
    req = Request(
        f"{base_url}{path}",
        data=body,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    with opener.open(req, timeout=15) as resp:
        raw = resp.read().decode("utf-8", errors="replace")
        return json.loads(raw)


def _check_windows_binary_unlocked(binary_path: Path, label: str) -> None:
    """Detect whether an existing Windows `.exe` is locked by a running process before rebuilding."""
    if os.name != "nt" or not binary_path.is_file():
        return
    try:
        with binary_path.open("r+b"):
            pass
    except OSError as exc:
        raise CheckError(
            f"Cannot rebuild `{label}` because `{binary_path}` is locked by a running process "
            f"({exc}). Close any open Dennett Desktop window (or stop the `{binary_path.name}` "
            f"process) and retry."
        ) from exc


def _resolve_catalog_default_model(head_http_url: str) -> str | None:
    """Query live runtime catalog so starter agents default to the runtime's primary model."""
    try:
        opener = build_opener(ProxyHandler({}))
        req = Request(f"{head_http_url}/v1/runtime/catalog", method="GET")
        with opener.open(req, timeout=10) as resp:
            payload = json.loads(resp.read().decode("utf-8", errors="replace"))
        default_id = payload.get("default_model_id")
        if isinstance(default_id, str) and default_id.strip():
            return default_id.strip()
        models = payload.get("models")
        if isinstance(models, list) and models:
            first_id = models[0].get("model_id")
            if isinstance(first_id, str) and first_id.strip():
                return first_id.strip()
    except Exception:
        return None
    return None


def _preseed_demo_d1_workspace(
    head_http_url: str,
    workspace_dir: Path,
    memory_scope: str,
) -> dict[str, str]:
    """Pre-seed a clean starter workspace, agent, and blank chat ready for immediate typing."""
    default_model_id = _resolve_catalog_default_model(head_http_url)
    project = _post_loopback_json(
        head_http_url,
        "/v1/projects",
        {
            "name": "Рабочее пространство",
            "workspace_path": str(workspace_dir),
            "memory_scope": memory_scope,
        },
    )
    agent_payload: dict[str, Any] = {
        "name": "Проектный агент",
        "description": "Проектный агент Dennett",
        "system_prompt": (
            f"Ты агент Dennett. Используй MCP-инструменты `dennett-memory` с областью "
            f"`{memory_scope}` для чтения и записи проектных решений, MCP-инструменты "
            f"`dennett-control` (`dennett_event_create`, `dennett_event_list`, "
            f"`dennett_event_update`, `dennett_event_cancel`) для создания и управления "
            f"расписаниями и отложенными событиями проекта, и сохраняй результаты в рабочей "
            f"папке проекта."
        ),
        "thinking_level": "medium",
        "allowed_memory_scopes": [memory_scope],
        "workspace_write_policy": {"mode": "allow_workspace"},
    }
    if default_model_id:
        agent_payload["model_preference"] = default_model_id
    agent = _post_loopback_json(head_http_url, "/v1/agents", agent_payload)
    session = _post_loopback_json(
        head_http_url,
        "/v1/sessions",
        {
            "agent_id": agent["agent_id"],
            "project_id": project["project_id"],
            "title": "Новый чат",
        },
    )
    return {
        "project_id": str(project["project_id"]),
        "agent_id": str(agent["agent_id"]),
        "session_id": str(session["session_id"]),
    }


def run_demo_d1(
    repo_root: Path,
    *,
    runtime: str,
    auth_profile: str,
    no_ui: bool,
    dry_run: bool,
) -> None:
    """Launch an isolated disposable D1 environment (PG + memoryd + Head + native Desktop UI)."""
    compose_file = repo_root / "ops" / "test-env" / "docker-compose.test.yml"
    if not compose_file.is_file():
        raise CheckError(f"Missing docker compose file: {compose_file}")

    session_tag = uuid.uuid4().hex[:8]
    project_name = f"dennett-demo-{session_tag}"
    temp_root = Path(tempfile.gettempdir()) / f"dennett-demo-d1-{session_tag}"
    profile_root = temp_root / "agy-profile"
    workspace_dir = temp_root / "workspace"
    memory_scope = "project:workspace"

    exe_suffix = ".exe" if os.name == "nt" else ""
    target_debug = repo_root / "target" / "debug"
    memoryd_bin = target_debug / f"dennett-memoryd{exe_suffix}"
    memory_mcp_bin = target_debug / f"dennett-memory-mcp{exe_suffix}"
    control_mcp_bin = target_debug / f"dennett-control-mcp{exe_suffix}"
    head_bin = target_debug / f"dennett-head{exe_suffix}"
    desktop_bin = target_debug / f"dennett-desktop{exe_suffix}"

    if dry_run:
        plan = {
            "mode": "demo-d1-dry-run",
            "isolated_profile_mode": "test_isolated",
            "compose_project": project_name,
            "compose_file": str(compose_file),
            "runtime": runtime,
            "auth_profile": auth_profile,
            "launch_ui": not no_ui,
            "temp_agy_profile_root": str(profile_root),
            "demo_workspace_dir": str(workspace_dir),
            "default_memory_scope": memory_scope,
            "dynamic_memory_hmac_enabled": True,
            "preseeded_workspace": {
                "project_name": "Рабочее пространство",
                "memory_scope": memory_scope,
                "agent_name": "Проектный агент",
                "session_title": "Новый чат",
            },
            "binaries": {
                "memoryd": str(memoryd_bin),
                "memory_mcp": str(memory_mcp_bin),
                "control_mcp": str(control_mcp_bin),
                "head": str(head_bin),
                "desktop": str(desktop_bin),
            },
            "v03_invariant": (
                "Closing the dennett-desktop window leaves dennett-head and dennett-memoryd "
                "running; press Ctrl+C in this launcher to stop the isolated demo stack."
            ),
        }
        print(json.dumps(plan, indent=2, ensure_ascii=False), flush=True)
        return

    if runtime == "antigravity":
        agy_bin = os.environ.get("DENNETT_AGY_BIN", "").strip() or "agy"
        if shutil.which(agy_bin) is None and not Path(agy_bin).is_file():
            raise CheckError(
                f"Antigravity CLI `{agy_bin}` not found on PATH. "
                "Install `agy >= 1.5.0` or pass `--runtime simulated` for offline UI inspection."
            )

    clean_env = build_tool_env(removals=("DENNETT_MEMORY_PG_URL", "DENNETT_CONTROL_PG_URL"))

    # Build server binaries (+ desktop binary with embedded dist when UI is enabled).
    run_cmd(
        [
            "cargo",
            "build",
            "--bin",
            "dennett-memoryd",
            "--bin",
            "dennett-memory-mcp",
            "--bin",
            "dennett-control-mcp",
            "--bin",
            "dennett-head",
            "--bin",
            "dennettctl",
        ],
        cwd=repo_root,
        env=clean_env,
    )
    if not no_ui:
        _check_windows_binary_unlocked(desktop_bin, "dennett-desktop")
        run_cmd(["pnpm", "--dir", "apps/desktop", "run", "build"], cwd=repo_root, env=clean_env)
        run_cmd(
            ["cargo", "build", "-p", "dennett-desktop", "--features", "custom-protocol"],
            cwd=repo_root,
            env=clean_env,
        )

    profile_root.mkdir(parents=True, exist_ok=True)
    workspace_dir.mkdir(parents=True, exist_ok=True)
    demo_token = f"dennett-demo-{secrets.token_hex(16)}"
    memory_hmac_secret = secrets.token_hex(32)
    auth_config_json = json.dumps(
        {
            "audience": "dennett-memoryd",
            "allowed_issuers": ["dennett-standalone", "dennett-head"],
            "hmac_secret": memory_hmac_secret,
            "tokens": [
                {
                    "token_id": f"tok-demo-{session_tag}",
                    "token": demo_token,
                    "principal_id": "dennett-demo-head",
                    "issuer": "dennett-standalone",
                    "allowed_scopes": [memory_scope, "project:default"],
                    "capabilities": ["read", "write"],
                    "revoked": False,
                }
            ],
        }
    )

    conv_before = count_real_agy_conversations()
    compose_base = ["docker", "compose", "-p", project_name, "-f", str(compose_file)]
    memoryd_proc: subprocess.Popen[str] | None = None
    head_proc: subprocess.Popen[str] | None = None
    ui_proc: subprocess.Popen[str] | None = None
    memoryd_http_url: str | None = None
    head_http_url: str | None = None

    try:
        run_cmd([*compose_base, "up", "-d", "--wait"], cwd=repo_root, env=clean_env)
        host_port = resolve_compose_host_port(repo_root, compose_file, project_name)
        mem_url = (
            f"postgres://dennett_memory_role:dennett_memory_test_pw"
            f"@127.0.0.1:{host_port}/dennett_memory_test"
        )
        ctrl_url = (
            f"postgres://dennett_control_role:dennett_control_test_pw"
            f"@127.0.0.1:{host_port}/dennett_control_test"
        )
        validate_test_pg_url("DENNETT_TEST_MEMORY_PG_URL", mem_url)
        validate_test_pg_url("DENNETT_TEST_CONTROL_PG_URL", ctrl_url)

        mem_env = build_tool_env(
            overrides={
                "DENNETT_TEST_PROFILE": "1",
                "DENNETT_TEST_MEMORY_PG_URL": mem_url,
                "DENNETT_MEMORY_AUTH_CONFIG": auth_config_json,
            },
            removals=("DENNETT_MEMORY_PG_URL", "DENNETT_MEMORY_AUTH_CONFIG_FILE"),
        )
        memoryd_proc = subprocess.Popen(
            [str(memoryd_bin), "--serve", "127.0.0.1:0"],
            cwd=repo_root,
            env=mem_env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        memoryd_addr = _read_listening_addr(memoryd_proc, "dennett-memoryd")
        memoryd_http_url = f"http://{memoryd_addr}"

        head_env = build_tool_env(
            overrides={
                "DENNETT_TEST_PROFILE": "1",
                "DENNETT_TEST_CONTROL_PG_URL": ctrl_url,
                "DENNETT_MEMORY_TOKEN": demo_token,
                "DENNETT_MEMORY_HMAC_SECRET": memory_hmac_secret,
            },
            removals=("DENNETT_CONTROL_PG_URL",),
        )
        head_proc = subprocess.Popen(
            [
                str(head_bin),
                "--serve",
                "127.0.0.1:0",
                "--runtime",
                runtime,
                "--auth-profile",
                auth_profile,
                "--profile-root",
                str(profile_root),
                "--memory-url",
                memoryd_http_url,
                "--memory-mcp-bin",
                str(memory_mcp_bin),
                "--control-mcp-bin",
                str(control_mcp_bin),
            ],
            cwd=repo_root,
            env=head_env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        head_addr = _read_listening_addr(head_proc, "dennett-head")
        head_http_url = f"http://{head_addr}"
        seeded = _preseed_demo_d1_workspace(head_http_url, workspace_dir, memory_scope)

        print("\n=== Dennett D1 Isolated Environment Ready ===", flush=True)
        print(f"  DENNETT_HEAD_URL      : {head_http_url}", flush=True)
        print(f"  DENNETT_MEMORY_URL    : {memoryd_http_url}", flush=True)
        print(f"  Runtime / Auth Profile: {runtime} / {auth_profile}", flush=True)
        print(f"  Workspace Path        : {workspace_dir}", flush=True)
        print(f"  Starter Project       : Рабочее пространство ({seeded['project_id']})", flush=True)
        print(f"  Starter Session       : Новый чат ({seeded['session_id']})", flush=True)
        print(f"  Memory Scope (HMAC)   : {memory_scope} (+ dynamic per-project scopes)", flush=True)
        print(f"  Isolated agy Profile  : {profile_root}", flush=True)
        print(
            "  V03 Note: Closing the Dennett window leaves Head & memoryd alive; "
            "re-run `dennett-desktop` with DENNETT_HEAD_URL or press Ctrl+C here to stop all demo services.\n",
            flush=True,
        )

        if not no_ui:
            ui_env = build_tool_env(overrides={"DENNETT_HEAD_URL": head_http_url})
            ui_proc = subprocess.Popen(
                [str(desktop_bin)],
                cwd=repo_root,
                env=ui_env,
                stdin=subprocess.DEVNULL,
            )

        print("[dennett-check] Demo running. Press Ctrl+C to shut down and clean up...", flush=True)
        while True:
            if head_proc.poll() is not None:
                raise CheckError(f"dennett-head exited unexpectedly (code={head_proc.returncode}).")
            time.sleep(0.5)
    except KeyboardInterrupt:
        print("\n[dennett-check] Ctrl+C received; tearing down isolated D1 demo...", flush=True)
    finally:
        _graceful_stop_daemon(ui_proc, "dennett-desktop")
        _graceful_stop_daemon(head_proc, "dennett-head", base_url=head_http_url)
        _graceful_stop_daemon(memoryd_proc, "dennett-memoryd", base_url=memoryd_http_url)
        run_cmd(
            [*compose_base, "down", "-v"],
            cwd=repo_root,
            env=clean_env,
            check=False,
            timeout_seconds=60,
        )
        shutil.rmtree(temp_root, ignore_errors=True)
        if runtime == "antigravity":
            conv_after = count_real_agy_conversations()
            if conv_before != conv_after:
                print(
                    f"[dennett-check] WARNING: ~/.gemini conversations count changed "
                    f"(before={conv_before}, after={conv_after})",
                    file=sys.stderr,
                    flush=True,
                )


def _preseed_demo_d2_workspace(
    head_http_url: str,
    workspace_dir: Path,
    memory_scope: str,
) -> dict[str, str]:
    """Pre-seed a clean D2 starter workspace with interactive `ask_before_write` policy."""
    default_model_id = _resolve_catalog_default_model(head_http_url)
    project = _post_loopback_json(
        head_http_url,
        "/v1/projects",
        {
            "name": "Рабочее пространство",
            "workspace_path": str(workspace_dir),
            "memory_scope": memory_scope,
        },
    )
    agent_payload: dict[str, Any] = {
        "name": "Проектный агент",
        "description": "Проектный агент Dennett (D2: интерактивные подтверждения, поручения и таймеры)",
        "system_prompt": (
            f"Ты агент Dennett. Используй MCP-инструменты `dennett-memory` с областью "
            f"`{memory_scope}` для чтения и записи проектных решений, MCP-инструменты "
            f"`dennett-control` (`dennett_event_create`, `dennett_event_list`, "
            f"`dennett_event_update`, `dennett_event_cancel`) для создания и управления "
            f"расписаниями и отложенными событиями проекта, и сохраняй результаты в рабочей "
            f"папке проекта."
        ),
        "thinking_level": "medium",
        "allowed_memory_scopes": [memory_scope],
        "workspace_write_policy": {"mode": "ask_before_write"},
    }
    if default_model_id:
        agent_payload["model_preference"] = default_model_id
    agent = _post_loopback_json(head_http_url, "/v1/agents", agent_payload)
    session = _post_loopback_json(
        head_http_url,
        "/v1/sessions",
        {
            "agent_id": agent["agent_id"],
            "project_id": project["project_id"],
            "title": "Новый чат",
        },
    )
    return {
        "project_id": str(project["project_id"]),
        "agent_id": str(agent["agent_id"]),
        "session_id": str(session["session_id"]),
    }


def run_demo_d2(
    repo_root: Path,
    *,
    runtime: str,
    auth_profile: str,
    no_ui: bool,
    dry_run: bool,
) -> None:
    """Launch an isolated disposable D2 environment (PG v4 + memoryd + Head scheduler/effects + Desktop UI)."""
    compose_file = repo_root / "ops" / "test-env" / "docker-compose.test.yml"
    if not compose_file.is_file():
        raise CheckError(f"Missing docker compose file: {compose_file}")

    session_tag = uuid.uuid4().hex[:8]
    project_name = f"dennett-demo-d2-{session_tag}"
    temp_root = Path(tempfile.gettempdir()) / f"dennett-demo-d2-{session_tag}"
    profile_root = temp_root / "agy-profile"
    workspace_dir = temp_root / "workspace"
    effects_ledger_path = temp_root / "effects-ledger.json"
    memory_scope = "project:workspace"

    exe_suffix = ".exe" if os.name == "nt" else ""
    target_debug = repo_root / "target" / "debug"
    memoryd_bin = target_debug / f"dennett-memoryd{exe_suffix}"
    memory_mcp_bin = target_debug / f"dennett-memory-mcp{exe_suffix}"
    control_mcp_bin = target_debug / f"dennett-control-mcp{exe_suffix}"
    head_bin = target_debug / f"dennett-head{exe_suffix}"
    desktop_bin = target_debug / f"dennett-desktop{exe_suffix}"

    if dry_run:
        plan = {
            "mode": "demo-d2-dry-run",
            "stage": "D2",
            "control_schema_version": 7,
            "isolated_profile_mode": "test_isolated",
            "compose_project": project_name,
            "compose_file": str(compose_file),
            "runtime": runtime,
            "auth_profile": auth_profile,
            "launch_ui": not no_ui,
            "temp_agy_profile_root": str(profile_root),
            "demo_workspace_dir": str(workspace_dir),
            "effects_ledger_path": str(effects_ledger_path),
            "default_memory_scope": memory_scope,
            "dynamic_memory_hmac_enabled": True,
            "preseeded_workspace": {
                "project_name": "Рабочее пространство",
                "memory_scope": memory_scope,
                "agent_name": "Проектный агент",
                "workspace_write_policy": "ask_before_write",
                "session_title": "Новый чат",
            },
            "binaries": {
                "memoryd": str(memoryd_bin),
                "memory_mcp": str(memory_mcp_bin),
                "control_mcp": str(control_mcp_bin),
                "head": str(head_bin),
                "desktop": str(desktop_bin),
            },
            "v03_invariant": (
                "Closing the dennett-desktop window leaves dennett-head, HeadScheduler, and "
                "dennett-memoryd running; press Ctrl+C in this launcher to stop the isolated D2 stack."
            ),
        }
        print(json.dumps(plan, indent=2, ensure_ascii=False), flush=True)
        return

    if runtime == "antigravity":
        agy_bin = os.environ.get("DENNETT_AGY_BIN", "").strip() or "agy"
        if shutil.which(agy_bin) is None and not Path(agy_bin).is_file():
            raise CheckError(
                f"Antigravity CLI `{agy_bin}` not found on PATH. "
                "Install `agy >= 1.5.0` or pass `--runtime simulated` for offline UI inspection."
            )

    clean_env = build_tool_env(removals=("DENNETT_MEMORY_PG_URL", "DENNETT_CONTROL_PG_URL"))

    # Build server binaries (+ desktop binary with embedded dist when UI is enabled).
    run_cmd(
        [
            "cargo",
            "build",
            "--bin",
            "dennett-memoryd",
            "--bin",
            "dennett-memory-mcp",
            "--bin",
            "dennett-control-mcp",
            "--bin",
            "dennett-head",
            "--bin",
            "dennettctl",
        ],
        cwd=repo_root,
        env=clean_env,
    )
    if not no_ui:
        _check_windows_binary_unlocked(desktop_bin, "dennett-desktop")
        run_cmd(["pnpm", "--dir", "apps/desktop", "run", "build"], cwd=repo_root, env=clean_env)
        run_cmd(
            ["cargo", "build", "-p", "dennett-desktop", "--features", "custom-protocol"],
            cwd=repo_root,
            env=clean_env,
        )

    profile_root.mkdir(parents=True, exist_ok=True)
    workspace_dir.mkdir(parents=True, exist_ok=True)
    demo_token = f"dennett-demo-d2-{secrets.token_hex(16)}"
    memory_hmac_secret = secrets.token_hex(32)
    auth_config_json = json.dumps(
        {
            "audience": "dennett-memoryd",
            "allowed_issuers": ["dennett-standalone", "dennett-head"],
            "hmac_secret": memory_hmac_secret,
            "tokens": [
                {
                    "token_id": f"tok-demo-d2-{session_tag}",
                    "token": demo_token,
                    "principal_id": "dennett-demo-d2-head",
                    "issuer": "dennett-standalone",
                    "allowed_scopes": [memory_scope, "project:default"],
                    "capabilities": ["read", "write"],
                    "revoked": False,
                }
            ],
        }
    )

    conv_before = count_real_agy_conversations()
    compose_base = ["docker", "compose", "-p", project_name, "-f", str(compose_file)]
    memoryd_proc: subprocess.Popen[str] | None = None
    head_proc: subprocess.Popen[str] | None = None
    ui_proc: subprocess.Popen[str] | None = None
    memoryd_http_url: str | None = None
    head_http_url: str | None = None

    try:
        run_cmd([*compose_base, "up", "-d", "--wait"], cwd=repo_root, env=clean_env)
        host_port = resolve_compose_host_port(repo_root, compose_file, project_name)
        mem_url = (
            f"postgres://dennett_memory_role:dennett_memory_test_pw"
            f"@127.0.0.1:{host_port}/dennett_memory_test"
        )
        ctrl_url = (
            f"postgres://dennett_control_role:dennett_control_test_pw"
            f"@127.0.0.1:{host_port}/dennett_control_test"
        )
        validate_test_pg_url("DENNETT_TEST_MEMORY_PG_URL", mem_url)
        validate_test_pg_url("DENNETT_TEST_CONTROL_PG_URL", ctrl_url)

        mem_env = build_tool_env(
            overrides={
                "DENNETT_TEST_PROFILE": "1",
                "DENNETT_TEST_MEMORY_PG_URL": mem_url,
                "DENNETT_MEMORY_AUTH_CONFIG": auth_config_json,
            },
            removals=("DENNETT_MEMORY_PG_URL", "DENNETT_MEMORY_AUTH_CONFIG_FILE"),
        )
        memoryd_proc = subprocess.Popen(
            [str(memoryd_bin), "--serve", "127.0.0.1:0"],
            cwd=repo_root,
            env=mem_env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        memoryd_addr = _read_listening_addr(memoryd_proc, "dennett-memoryd")
        memoryd_http_url = f"http://{memoryd_addr}"

        head_env = build_tool_env(
            overrides={
                "DENNETT_TEST_PROFILE": "1",
                "DENNETT_TEST_CONTROL_PG_URL": ctrl_url,
                "DENNETT_MEMORY_TOKEN": demo_token,
                "DENNETT_MEMORY_HMAC_SECRET": memory_hmac_secret,
            },
            removals=("DENNETT_CONTROL_PG_URL",),
        )
        head_proc = subprocess.Popen(
            [
                str(head_bin),
                "--serve",
                "127.0.0.1:0",
                "--runtime",
                runtime,
                "--auth-profile",
                auth_profile,
                "--profile-root",
                str(profile_root),
                "--effects-ledger",
                str(effects_ledger_path),
                "--memory-url",
                memoryd_http_url,
                "--memory-mcp-bin",
                str(memory_mcp_bin),
                "--control-mcp-bin",
                str(control_mcp_bin),
            ],
            cwd=repo_root,
            env=head_env,
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        head_addr = _read_listening_addr(head_proc, "dennett-head")
        head_http_url = f"http://{head_addr}"
        seeded = _preseed_demo_d2_workspace(head_http_url, workspace_dir, memory_scope)

        print("\n=== Dennett D2 Isolated Environment Ready ===", flush=True)
        print(f"  DENNETT_HEAD_URL      : {head_http_url}", flush=True)
        print(f"  DENNETT_MEMORY_URL    : {memoryd_http_url}", flush=True)
        print(f"  Runtime / Auth Profile: {runtime} / {auth_profile}", flush=True)
        print(f"  Workspace Path        : {workspace_dir}", flush=True)
        print(f"  Effects Ledger        : {effects_ledger_path}", flush=True)
        print(f"  Starter Project       : Рабочее пространство ({seeded['project_id']})", flush=True)
        print(f"  Starter Session       : Новый чат ({seeded['session_id']})", flush=True)
        print("  Write Policy          : ask_before_write (Спрашивать запись)", flush=True)
        print(f"  Memory Scope (HMAC)   : {memory_scope} (+ dynamic per-project scopes)", flush=True)
        print(f"  Isolated agy Profile  : {profile_root}", flush=True)
        print(
            "  V03 Note: Closing the Dennett window leaves Head, HeadScheduler & memoryd alive; "
            "re-run `dennett-desktop` with DENNETT_HEAD_URL or press Ctrl+C here to stop all demo services.\n",
            flush=True,
        )

        if not no_ui:
            ui_env = build_tool_env(overrides={"DENNETT_HEAD_URL": head_http_url})
            ui_proc = subprocess.Popen(
                [str(desktop_bin)],
                cwd=repo_root,
                env=ui_env,
                stdin=subprocess.DEVNULL,
            )

        print("[dennett-check] D2 demo running. Press Ctrl+C to shut down and clean up...", flush=True)
        while True:
            if head_proc.poll() is not None:
                raise CheckError(f"dennett-head exited unexpectedly (code={head_proc.returncode}).")
            time.sleep(0.5)
    except KeyboardInterrupt:
        print("\n[dennett-check] Ctrl+C received; tearing down isolated D2 demo...", flush=True)
    finally:
        _graceful_stop_daemon(ui_proc, "dennett-desktop")
        _graceful_stop_daemon(head_proc, "dennett-head", base_url=head_http_url)
        _graceful_stop_daemon(memoryd_proc, "dennett-memoryd", base_url=memoryd_http_url)
        run_cmd(
            [*compose_base, "down", "-v"],
            cwd=repo_root,
            env=clean_env,
            check=False,
            timeout_seconds=60,
        )
        shutil.rmtree(temp_root, ignore_errors=True)
        if runtime == "antigravity":
            conv_after = count_real_agy_conversations()
            if conv_before != conv_after:
                print(
                    f"[dennett-check] WARNING: ~/.gemini conversations count changed "
                    f"(before={conv_before}, after={conv_after})",
                    file=sys.stderr,
                    flush=True,
                )


def build_parser() -> argparse.ArgumentParser:
    """Construct the CLI argument parser for `dennett-check`."""
    parser = argparse.ArgumentParser(
        prog="dennett-check",
        description="Unified Dennett verification and test runner (D0+).",
    )
    sub = parser.add_subparsers(dest="command", required=True)

    sub.add_parser("docs", help="Validate required docs, local markdown links, and runtime refs.")
    sub.add_parser("deps", help="Verify Cargo.lock, toolchain pin, and crate dependency boundaries.")
    sub.add_parser(
        "fast",
        help="Run desktop frontend checks, rustfmt, strict clippy, rustdoc, doctests, and fast unit/safety tests.",
    )
    sub.add_parser(
        "self-check",
        help="Prove 0-test runs are rejected, intentional failures fail, guards fail closed, and DSN overrides are blocked.",
    )

    p_int = sub.add_parser(
        "integration",
        help="Run real PostgreSQL isolation, standalone memoryd, Head, and desktop V03 bridge tests.",
    )
    p_int.add_argument(
        "--manage-pg",
        action="store_true",
        help="Start and tear down ops/test-env/docker-compose.test.yml on an ephemeral loopback port.",
    )

    sub.add_parser(
        "runtime-probe",
        help="Run live Google Antigravity (`agy acp`) D0/D1 compatibility probes (A01–A06, A09, V04).",
    )

    p_demo = sub.add_parser(
        "demo-d1",
        help="Start a safe isolated D1 trial (ephemeral PG + memoryd + Head + native Desktop UI).",
    )
    p_demo.add_argument(
        "--runtime",
        choices=("antigravity", "simulated"),
        default="antigravity",
        help="Runtime adapter for dennett-head (default: antigravity).",
    )
    p_demo.add_argument(
        "--auth-profile",
        choices=("oauth-personal", "gemini-api-key"),
        default="oauth-personal",
        help="Auth profile for dennett-head (default: oauth-personal for Google AI Ultra).",
    )
    p_demo.add_argument(
        "--no-ui",
        action="store_true",
        help="Start PG + memoryd + Head only and print DENNETT_HEAD_URL without opening the desktop window.",
    )
    p_demo.add_argument(
        "--dry-run",
        "--print-plan",
        dest="dry_run",
        action="store_true",
        help="Print the isolated demo launch plan as JSON and exit without starting processes.",
    )

    p_demo_d2 = sub.add_parser(
        "demo-d2",
        help="Start a safe isolated D2 trial (ephemeral PG v4 + memoryd + Head scheduler/effects + native Desktop UI).",
    )
    p_demo_d2.add_argument(
        "--runtime",
        choices=("antigravity", "simulated"),
        default="antigravity",
        help="Runtime adapter for dennett-head (default: antigravity).",
    )
    p_demo_d2.add_argument(
        "--auth-profile",
        choices=("oauth-personal", "gemini-api-key"),
        default="oauth-personal",
        help="Auth profile for dennett-head (default: oauth-personal for Google AI Ultra).",
    )
    p_demo_d2.add_argument(
        "--no-ui",
        action="store_true",
        help="Start PG + memoryd + Head only and print DENNETT_HEAD_URL without opening the desktop window.",
    )
    p_demo_d2.add_argument(
        "--dry-run",
        "--print-plan",
        dest="dry_run",
        action="store_true",
        help="Print the isolated D2 demo launch plan as JSON and exit without starting processes.",
    )

    p_all = sub.add_parser(
        "all",
        help="Run docs, deps, fast, self-check, and integration sequentially.",
    )
    p_all.add_argument(
        "--manage-pg",
        action="store_true",
        help="Start and tear down ops/test-env/docker-compose.test.yml for integration.",
    )

    return parser


def main(argv: Sequence[str] | None = None) -> int:
    """Entry point for `dennett-check`."""
    _configure_stdio_utf8()
    parser = build_parser()
    args = parser.parse_args(argv)

    try:
        repo_root = find_repo_root()
        match args.command:
            case "docs":
                check_docs(repo_root)
            case "deps":
                check_deps(repo_root)
            case "fast":
                check_fast(repo_root)
            case "self-check":
                check_self_check(repo_root)
            case "integration":
                check_integration(repo_root, manage_pg=args.manage_pg)
            case "runtime-probe":
                check_runtime_probe(repo_root)
            case "demo-d1":
                run_demo_d1(
                    repo_root,
                    runtime=args.runtime,
                    auth_profile=args.auth_profile,
                    no_ui=args.no_ui,
                    dry_run=args.dry_run,
                )
            case "demo-d2":
                run_demo_d2(
                    repo_root,
                    runtime=args.runtime,
                    auth_profile=args.auth_profile,
                    no_ui=args.no_ui,
                    dry_run=args.dry_run,
                )
            case "all":
                check_docs(repo_root)
                check_deps(repo_root)
                check_fast(repo_root)
                check_self_check(repo_root)
                check_integration(repo_root, manage_pg=args.manage_pg)
            case other:
                raise CheckError(f"Unknown command: {other}")
    except CheckError as exc:
        print(f"[dennett-check] ERROR: {exc}", file=sys.stderr, flush=True)
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
