# WP-M02-004 capture plan — workspace snapshots and file effects

```yaml
blog_capture:
  story_candidate: true
  possible_angle: "Почему Undo для агента начинается не с кнопки, а с доказательства того, какой файл он увидел"
  shots:
    - "version-bound mutation timeline: base, unrelated human edit, safe agent edit"
    - "conflict fixture where a human changes the same file before publication"
    - "restart fixture ending in RecoveryRequired instead of invented success"
  quotes_to_preserve:
    - source: owner-direction
      ref: "simplify only where quality does not fall; discuss business logic before encoding it"
  privacy_risks:
    - "workspace contents and absolute local paths"
    - "checkpoint before-images may contain private project data"
    - "test fixtures must contain synthetic content only"
```

## Starting facts — 2026-07-22

- `WP-M02-003` merged as `76f7d639addca2d043b9143b23e0154914e479b4` after public Linux and Windows gates passed.
- The focused pre-change baseline passed for `dennett-effect-core`, `dennett-node`, `dennett-head` and `dennett-storage-sqlite`; repository and planning verification were also green.
- This package is deliberately internal. The current desktop cannot honestly demonstrate snapshot identity, crash recovery or same-file conflict prevention; the owner-visible Files/Changes flow is designed later in WP-M02-007 and implemented in WP-M02-008.

## Behavior fixed before code

1. One interactive writer remains the simple default. Leases or worktrees appear only for actual competing writers or explicit isolation.
2. Every mutation names a known base revision, but conflict detection is per touched path. An unrelated human edit is retained rather than treated as permission to overwrite or as a reason to discard all work.
3. A touched file that no longer matches the base blocks the whole multi-file publication before any new change is applied.
4. `.git/**` and `.dennett/project.json` are outside the generic file-effect path. Their dedicated owners remain Git integration and project registration.
5. Durable intent precedes filesystem publication. After a crash, current bytes must match a recorded before-image or after-image; any third state becomes `RecoveryRequired`.
6. A checkpoint restore never claims to undo external effects and never overwrites a newer unrecognized edit on a touched path.

## Evidence to collect

- normal add/modify/delete/rename through the real Node-owned adapter;
- path traversal, symlink/junction and protected-path denial;
- disjoint external edit preserved while the requested file change succeeds;
- same-path external edit rejected with no partial publication;
- crash after one file of a multi-file operation and restart reconciliation;
- checkpoint compare and restore, including refusal when a human changed one touched file;
- SQLite reopen proving revision, claim, receipt and checkpoint durability;
- detached R3 findings and repairs, including any simplification forced by DRY/KISS/YAGNI review.

## Publication state

This is capture material, not a published claim. A standalone field note is justified only if implementation or review yields a concrete failure mode or reusable lesson; otherwise the evidence rolls into the M02 milestone chronicle.

## Implementation turns — 2026-07-22 to 2026-07-23

- The first complete model used five durable ideas rather than a generic
  filesystem abstraction: a bounded manifest, path transitions, one operation
  journal, immutable checkpoints and a monotonic resulting revision.
- A detached pre-implementation critic rejected three tempting shortcuts. An
  “exact snapshot” now means the entire explicitly bounded project namespace;
  exceeding the entry, depth, per-file or total-byte bound rejects the
  observation. Terminal receipts are never reinterpreted after a later human
  edit. Multi-file publication stages every after-image and retains displaced
  before-images until reconciliation proves the result.
- A real SQLite-backed restore test exposed an atomicity bug after the files
  were already correct: the operation tried to reference a resulting revision
  before that snapshot was durably visible. Snapshot publication and terminal
  success now share one SQLite transaction.
- The recovery test was then made less polite. It durably prepares a two-file
  operation, publishes exactly one file, simulates process loss before
  classification, recreates the application over the same SQLite database and
  asks startup reconciliation what happened. The answer is
  `RecoveryRequired`, not success or a blind retry. Restoring the automatic
  checkpoint returns only the touched file while preserving an unrelated human
  edit.
- Filesystem authority is handle-relative and no-follow. The adversarial set
  now covers lexical escapes, protected `.git` and project-identity paths,
  linked parents, a root whose durable identity belongs to another directory,
  Windows ASCII and Unicode case aliases, alternate streams and ambiguous
  suffixes. Linux mount identity uses `statx` mount IDs so a bind or nested
  mount cannot hide behind the same device number.
- The generic mutation path deliberately rejects a reversible before-image
  above 16 MiB instead of accepting an operation it cannot restore. A complete
  manifest may hash files up to 2 GiB each and 8 GiB total without retaining
  their bytes.
- A final self-audit turned "unrelated edits are preserved" into a missing
  executable example. SQLite had wrongly required the automatic checkpoint to
  equal the caller's older base, rejecting a valid operation after an unrelated
  human edit. The operation now remains bound to its original base while the
  safety checkpoint captures the newer workspace, so both edits survive.
- Detached review then found the uncomfortable filesystem edges: Unix `0600`,
  Windows ACLs and alternate streams, 8.3 aliases, regular-file bind mounts,
  aggregate recovery size, unbounded startup verification and a crash between
  moving the old file aside and publishing its replacement. Exact Unix modes
  now round-trip; unsupported xattr/ACL/stream metadata fails closed; paths and
  mounts are fenced; both image sets share one 64 MiB budget; verification is
  paged; and the micro-crash restores after a real SQLite close and reopen.
- Re-review caught four places where the first repair was still only nearly
  safe. Metadata is now checked again immediately before publication and after
  the original file is moved, with restoration on mismatch; final cleanup also
  refuses to delete a backup that changed after validation. Windows compares
  every opened component with its canonical long name instead of outlawing
  legitimate `~1` filenames. SQLite keyset scans are sargable, and blob
  relations stream through exact-membership, count and 64 MiB bounds.
- A later detached review found two final identity mistakes. Exact Unix mode
  was preserved on disk but omitted from command-intent hashing, so requests
  for `0600` and `0640` could be mistaken for one idempotent command. More
  seriously, cleanup inferred ownership of adjacent temporary files from a
  deterministic name plus matching bytes. A coincidental user file could
  therefore be deleted. The publication protocol now has one private staging
  directory per operation, a 256-bit durable nonce, and durable OS identity
  receipts for the directory, marker, every after-image and every before-image.
  Head persists those receipts before Node may change a user path. Cleanup
  removes only receipt-matching identities, refuses unknown entries and never
  recurses. Tests preserve both a legacy name collision and a same-byte staged
  replacement, demonstrating why content equality is evidence of state but
  never evidence of ownership.
- The same-byte replacement test then failed only on Linux CI. Removing and
  immediately recreating the staged file allowed the filesystem to recycle the
  just-freed inode, defeating a receipt made only from device and inode
  numbers. The fix did not weaken the assertion: every before- and after-image
  now has a private hard-link witness. That witness pins the original inode
  across process restarts until cleanup, so deletion plus recreation cannot
  impersonate the staged object even when bytes and permissions are identical.
  The staging directory is durably synchronized before its receipt is returned,
  keeping the proof ordered ahead of any user-path publication after a crash.
- A later Linux run failed for a less profound reason: a privacy test searched
  the entire debug string for the two characters `5a`, which can naturally
  occur in a random project UUID. It now compares against the complete debug
  representation of the seeded secret identity, preserving the privacy claim
  without probabilistic false alarms.
- The owner accepted this as an internal checkpoint because the existing M01
  desktop has no truthful control for inspecting snapshot identity or crash
  reconstruction. The visual owner gate remains WP-M02-007/WP-M02-008.

## Current evidence snapshot

- Focused Rust qualification passes the effect-core, Head, Node and SQLite
  suites, including real temporary files and the real SQLite journal.
- Node startup runs workspace reconciliation after project-location recovery
  and before accepting local commands.
- Clippy passes with warnings denied for all four touched Rust packages.
- The catalogue cases remain milestone-spanning rather than being marked
  prematurely automated: command/test receipts continue in WP-M02-005 and the
  owner-facing review flow continues in WP-M02-007/WP-M02-008.
