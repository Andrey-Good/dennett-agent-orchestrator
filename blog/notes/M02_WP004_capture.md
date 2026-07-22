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
