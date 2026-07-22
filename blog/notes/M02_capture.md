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
- 2026-07-22 — The first diagnostic lifecycle test failed on Windows because locking the JSON marker also prevented another process from reading it. The design changed to a readable marker plus a separate locked file; restart tests then distinguished a live process from a stale crash marker.
- 2026-07-22 — A detached real Node was killed and restarted in tests. The next process retained the earlier logs, labelled the previous exit `unclean`, and did not persist the project path. A blocked diagnostics directory also proved that logging failure does not prevent Node startup.
- 2026-07-22 — Two detached closure reviews blocked the first implementation. They found that a lossless logging queue could freeze Node on a bad disk, concurrent startup could lose lifecycle evidence on Windows, a truncated marker could disable diagnostics permanently, and adapter-host failures collapsed into one generic fence.
- 2026-07-22 — The repair changed the writer to a bounded non-blocking queue with visible drop counts, gave every process run a UUID, made lifecycle publication atomic, recovered corrupt markers, bounded logs by age and bytes, and introduced a strict adapter-host stderr code channel. A 48-thread lifecycle race and two real child-process stderr tests now pass.

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
- WP-M02-001 targeted tests: privacy-safe log persistence, token-shaped secret rejection, lifecycle retention, 48-way concurrent startup, corrupt-marker recovery, subscriber-init rollback, handled startup failure, abrupt detached-Node termination/restart, adapter-host stderr classification and diagnostics-degraded startup all pass.

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
- Persistent rotating logs, crash markers and a local diagnostic summary are implemented in WP-M02-001; support-bundle export and a desktop Diagnostics workspace remain later work.
- The default Git integration policy must distinguish local reversible work from remote consequential effects.

## Closure-review addendum

- Two fresh closure reviewers still rejected the green implementation. The
  32 MiB bound was accidentally a lifetime quota, the crash marker did not
  actually contain the promised last durable phase, a terminal chat failure
  lost its safe cause code, and the adapter-host deadline started only after
  stdin had been written. These were semantic failures that ordinary
  happy-path tests had not exposed.
- The second repair replaced the lifetime counter with locked daily-and-size
  rotation, persisted immutable lifecycle checkpoints, flushed the
  asynchronous queue before final drop accounting, retained normalized chat
  failure codes, cleaned orphan lock/temp/checkpoint files and moved the
  adapter-host deadline around both request writing and response waiting.
- New tests force quota rollover, a physical log-write failure, a genuinely
  blocked child stdin, last-phase recovery after a crash, corrupt-latest-record
  honesty, orphan cleanup, safe run-ID recovery and a spoofed private tracing
  target.

## Second closure-review addendum

- A later privacy and reliability pass found that bounded retention was not
  enough. Path checks still happened before filesystem mutation, so a
  directory link or rename could redirect later I/O; lifecycle inspection was
  bounded by file size but not by entry count; and publishing a checkpoint
  from the application thread could still wait on disk or a lock.
- The repair moved managed diagnostic I/O behind open directory capabilities
  with no-follow child opens, private per-user permissions and bounded reads,
  enumeration and maintenance locks. The public `record` path now only updates
  atomics and a one-slot wake channel; one integration test holds the lifecycle
  lock while publishing events and verifies that the caller returns promptly.
- Shutdown now has a five-second caller deadline and writes whether queue flush
  and the final drop count were actually complete. Runs use a monotonic
  sequence rather than wall-clock order, and `doctor` reports corrupt,
  wrong-type or noncanonical entries as degraded/unknown instead of letting an
  older clean result masquerade as current.
- The same review exposed a separate adapter-host race: a response admitted
  just before a fence could become visible after the host was declared failed.
  Admission, generation and fencing now share one coordination boundary, with
  barrier tests for both request admission and buffered-response publication.
- At this checkpoint, 30 observability unit/integration tests and 10 focused
  runtime-host tests pass. The package remains open until the full repository
  gate and another detached review pass; a green local suite is evidence, not
  permission to declare the reliability problem solved.

## Third closure-review addendum

- The next independent pass justified keeping the closure gate. It found four
  failure sequences that a green happy-path suite had not exposed: a full
  supervisor pipe could block a synchronous console writer; an unknown host
  response could try to acquire its own coordination lock; a clock rollback of
  more than one day could invalidate the very records needed to preserve
  monotonic ordering; and an unsafe `DENNETT_DATA_DIR` could be treated as a
  harmless diagnostics failure before Node opened canonical SQLite state.
- The repaired event path is non-blocking for both disk and console sinks. The
  host dispatcher releases coordination before fencing, and a real subprocess
  fixture proves that an unknown response reaches a bounded failed state rather
  than deadlocking. Lifecycle validity no longer depends on today's wall clock;
  sequence ordering survives a 48-hour rollback fixture, and a corrupt newest
  record can no longer borrow a reassuring drop-count claim from an older run.
- Node now retains a no-follow capability for its data root and rejects relative
  or linked roots before opening canonical state. A blocked diagnostics child
  can still degrade safely to console-only evidence, preserving the intended
  difference between "logging is unavailable" and "the storage root is unsafe."
- The focused checkpoint now passes 36 observability tests, 11 runtime-host
  tests and a dedicated fail-closed Node data-root test. The real subscription
  canaries remain a separate closure gate because deterministic CI cannot own
  the user's ChatGPT login or distinguish a code regression from an external
  provider outage.
- The separate live gate was then run explicitly against the owner's ChatGPT
  subscription. Both scenarios passed in 84 seconds: one continued the same
  Codex session after a real Node restart, and one steered the same active turn
  without replacing it or issuing a hidden Stop.

## Fourth closure-review addendum

- A final privacy pass found that `doctor --json` still serialized the absolute
  diagnostics path even though the human-readable form hid it. That path adds
  no diagnostic value—the caller just supplied it—and could disclose a Windows
  account name when support output is copied. The field remains available to
  trusted in-process callers but is now excluded from JSON, with a CLI
  regression that rejects both the key and the temporary profile path.
- A corrupt terminal filename also reserved no run sequence because allocation
  trusted only readable JSON. The next run could reuse the number and make
  ordering depend on UUID text. Canonically shaped filenames now reserve their
  sequence even when their contents are unreadable; a regression writes a
  corrupt sequence 2 and proves that the next clean run receives sequence 3.
- Node's data-root capability now participates in startup instead of serving as
  a one-time check: it creates direct child directories and lock/database files
  without following links, rejects preplanted SQLite sidecar links, and verifies
  root identity around the path-based SQLx open. An intermediate-link fixture,
  a directory-swap fixture and a preplanted-database-link Node test cover the
  boundary. The focused checkpoint now passes 39 observability tests and all 15
  Node unit/runtime-host tests before the full gate is rerun.
- Because the repair changed Node startup before SQLite opened, the two real
  subscription canaries were rerun rather than inherited from the prior commit.
  Both passed again: same-turn native steering and continuation of the same
  Codex session across a real Node restart.

## Fifth closure-review addendum

- The first component-by-component root test was not adversarial enough: the
  redirected leaf did not exist. A reviewer supplied the missing counterexample.
  When the whole remainder already existed behind an intermediate link,
  `symlink_metadata` could accept it as the starting ancestor. Root opening and
  inspection now begin at the filesystem anchor and traverse every component
  with no-follow handles. Existing system ancestors are opened read-only and
  never have their ACLs rewritten; only the created/private subtree and final
  profile root are secured.
- SQLx still requires a filename. A post-open identity comparison could detect
  a Unix rename but only after SQLite had written through the replaced path.
  Windows already pins the directory against rename; Linux now gives SQLx a
  `/proc/self/fd/<directory-handle>/control.sqlite3` path, proven separately to
  keep writes in the opened directory after its display path is replaced.
  Unsupported platforms fail closed until their SQLite adapter has an
  equivalent capability-relative open.
- Run sequence allocation now snapshots the highest readable or canonically
  named observation before startup cleanup. Reconciliation consumes numbers
  from that one monotonic allocator, so deleting an unreadable orphan checkpoint
  cannot make its run number available again. The focused suite now passes 40
  observability tests and 15 Node unit/runtime-host tests.

## Sixth closure-review addendum

- The next reviewer found that the sequence floor above still lived only in
  memory. If cleanup deleted a corrupt high-number checkpoint and startup then
  rolled back, the following process could forget that number. Each reservation
  now advances one privacy-safe durable high-water marker before destructive
  cleanup or active-marker publication. A double-failure regression removes the
  corrupt source, rolls startup back, and proves the next run still advances;
  exhaustion at the integer boundary now remains permanently fail-closed too.

## Seventh closure-review addendum

- A crash during high-water publication could still leave a uniquely named
  temp file before normal cleanup. Hundreds of identical failures would then
  fill the intentionally bounded directory scan. Lifecycle writes are already
  serialized by a per-component maintenance lock, so they now reuse one fixed
  private temp name. An interrupted floor write is replaced on the next start;
  repeated crashes can leave one orphan, not an ever-growing collection.
