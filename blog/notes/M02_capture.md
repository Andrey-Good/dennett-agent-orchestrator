---
milestone: "M02"
status: "active"
story_candidate: true
possible_angle: "The agent could already read files; M02 began when reading stopped being confused with safe project work"
privacy_risks:
  - "local absolute paths in diagnostics and screenshots"
  - "repository content or provider session identifiers in traces"
---

# Capture: M02

## Chronology

- 2026-07-22 — The owner approved M02 after challenging the phrase "first Codex work cycle": M01 already proved a live request that read a file and returned an answer. The milestone boundary was corrected to a controlled file-change, verification and review cycle.
- 2026-07-22 — The owner made diagnosability a primary concern: failures must leave enough evidence to locate the failed layer and repair the cause.
- 2026-07-22 — A clean worktree was created from `origin/main` at `924bbcc` on branch `codex/m02-project-workspace`.
- 2026-07-22 — The first `just check` attempt passed Rust, Python and protocol checks but could not start TypeScript because the fresh worktree had no `node_modules`. After a frozen `pnpm install`, the complete gate passed in 237.7 seconds.
- 2026-07-22 — An independent architecture review agreed with the diagnostics-first sequence but found a dangerous inherited shortcut: M01 derived project identity from the current folder path. M02 planning was corrected before implementation and an explicit owner decision was opened.

## Decision turns

- Hypothesis: M02 needed the first working Codex cycle. Objection: the owner had already exercised file reading through the real chat. Change: define M02 around authoritative workspace state, controlled mutations, version-bound tests, diff review and recovery.
- Hypothesis: the existing trace correlation was sufficient for debugging. Evidence: `dennett-observability` currently initializes console tracing but does not persist rotating logs or crash evidence. Change: make a privacy-safe local diagnostic baseline an early M02 package rather than postponing all observability to production hardening.
- Hypothesis: Git meant commit/push integration. Clarification: Git first supplies base identity, isolation, conflict detection and rollback. Remote push and pull-request creation remain guarded effects and are not allowed to define the core M02 exit gate.
- Hypothesis: a canonicalized path could continue to identify a project. Counterexample: moving the folder would split one project into two identities, while reusing a path could inherit the wrong history or trust. Recommendation: stable generated Project ID plus a separate relocatable WorkspaceBinding.

## Measurements and tests

- Pre-change quick gates: repository, documentation, planning and generated documentation checks passed.
- Pre-change full gate: `mise exec -- just check` passed after installing the lockfile-pinned Node dependencies.
- Full gate wall time after dependency installation: 237.7 seconds.
- Live-provider tests remained intentionally ignored in the credential-free gate; deterministic fake/runtime, restart, cancellation, SQLite and IPC tests passed.

## Visual candidates

- A trace diagram showing one project command crossing UI, Node, Head, Codex, filesystem, tests and Git receipts.
- Before/after screenshot: fixture-only right panel versus real diff/test/checkpoint evidence.
- Failure reconstruction: interrupted operation found on restart with the exact last durable phase.

## Quotes worth preserving

- Owner: "Меня больше стабильность волнует, если случиться какая то ошибка, сможешь ли по логам узнать ошибку и ее причину, чтобы исправить."

## Known limitations and open threads

- The M02 Work Packages and 28-case acceptance catalogue are specified but not yet implemented.
- `DEC-0006` is intentionally open: implementation of project identity and lifecycle cannot begin until the owner accepts or rejects the stable-ID recommendation.
- The exact Files/Changes/Diff layout requires an owner-approved Figma checkpoint before implementation.
- Persistent rotating logs, crash markers, a diagnostic summary and support bundle are planned, not yet implemented.
- The default Git integration policy must distinguish local reversible work from remote consequential effects.
