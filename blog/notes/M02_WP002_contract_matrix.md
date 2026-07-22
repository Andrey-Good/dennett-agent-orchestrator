---
milestone: "M02"
work_package: "WP-M02-002"
status: "working-note"
publish_directly: false
---

# M02 workspace contract matrix

This note records the semantic boundary agreed before the public protocol is
edited. It is implementation evidence and future blog source, not a second
product specification.

## Ownership and identity

| Concern | Authoritative owner | Stable identity | Explicitly not identity |
|---|---|---|---|
| Project | Node project registry | generated `ProjectId` | display name or current path |
| Local location | Node Workspace Manager | `WorkspaceBindingId` | portable metadata alone |
| Exact file state | Node Workspace Manager | binding + monotonic revision + snapshot ID | wall-clock time |
| Access policy | local Dennett profile / Trust boundary | project ID + policy revision | `.dennett`, instructions, model output or source fingerprint |
| Portable project state | project folder | versioned `.dennett` identity and shared memory | personal memory, chats, secrets or provider settings |

Moving a folder changes or repairs its local binding; it does not create a new
logical project. Ordinary source edits advance the workspace revision but do
not reset the user's local project policy.

## Existing-folder admission

1. Inspect the selected location read-only.
2. Return an expiring inspection identity, detected source features and the
   observed `.dennett` state.
3. If `.dennett` is absent, expose that minimal portable structure creation is
   available. Do not create it during inspection.
4. Registration must state one explicit action: use valid portable metadata,
   create the minimal structure, keep the project local-only, or explicitly
   fork a copied portable identity with a new Project ID.
5. Revalidate the inspection before committing the registration.
6. A portable project ID can suggest identity but can never import local access
   rights. A new installation has no project policy until its user chooses one.

## Command lifecycle

- Queries carry the authenticated client-session identity.
- Every mutation carries the accepted M01 `CommandMetadata`, including command,
  idempotency, correlation and authority-epoch fields.
- Unary mutation responses acknowledge durable admission only.
- Completion is represented by a typed receipt and watch projection; admission
  never claims that a file, command, test, checkpoint or review action finished.
- Cancellation targets one workspace operation. If termination cannot be
  proven, the result is recovery-required rather than falsely cancelled.

## Workspace evidence

| Evidence | Must identify |
|---|---|
| File change | project, binding, base revision, resulting revision and project-relative path |
| Diff | exact from/to revisions and a bounded content/evidence handle |
| Command receipt | command/correlation/operation IDs, observed revision, times and terminal classification |
| Test receipt | source command, exact verified revision, outcome and stale state |
| Artifact | real project-relative output, source command and producing revision |
| Checkpoint | base/current revisions, touched paths, artifacts, external-effect references and provider continuation handle |
| Review | exact revision, comments, tests and explicit review action |

The public protocol carries project-relative paths and opaque handles. It does
not expose Rust filesystem objects, Git CLI output, database rows or Codex SDK
payloads.

## Failure taxonomy

Workspace failures remain distinguishable as stale snapshot, scope denial,
conflict, cancellation, missing location, retryable adapter failure, terminal
adapter failure, validation failure or recovery-required. The common M01 error
envelope remains intact and supplies the safe user message and correlation ID.

## Compatibility and scope guard

- Keep every accepted M01 field number, method and package unchanged.
- Add a new workspace contract and additive ProjectService methods.
- Generate Rust and TypeScript clients from Protobuf; never edit generated files.
- Do not add project storage, filesystem execution, Git behavior or UI fixtures
  in this package.
- Do not model managed Runs, remote publication, arbitrary connectors or a full
  IDE protocol.

## Implementation observations

- The first repository-wide quality gate rejected two generated Rust enums:
  embedding a complete `WorkspaceSnapshot` made their stack representation too
  large. The repair was made in the pinned Protobuf generator configuration,
  which boxes the affected variants without changing the wire contract; no
  generated file was hand-edited and no schema was distorted for one language.
- The completed local gate covers exact descriptor shape, Buf lint, generated
  Rust/TypeScript freshness, additive compatibility with `origin/main`, a
  deliberately breaking negative probe, contract value types and the complete
  existing repository suite. After detached review, the focused generator
  suite has 17 tests and the contract crate has seven new tests.
- The post-review branch-wide gross diff is 10,059 lines against a 12,000-line
  package cap. About 65% of that diff is generated Rust and TypeScript, useful
  context when this package later becomes part of the M02 development story.

## Detached-review corrections

Two independent read-only reviewers rejected the first green implementation.
Their useful findings changed the public contract before it could become
expensive to repair:

- trust elevation now carries a Trust-issued decision reference and an
  optimistic policy revision; `UNSPECIFIED` is fail-closed rather than an
  accidental grant;
- copied portable identity now has an explicit fork action, while preserving
  the existing ID imports no local access policy;
- the legacy M01 `CreateProject` RPC remains wire-compatible but is deprecated
  and normatively limited to safe empty-project compatibility, so an existing
  folder cannot bypass `.dennett` inspection;
- one canonical operation ID replaces two competing IDs, and cancellation
  identifies its own command separately from the target operation;
- shared receipt validators exhaustively reject contradictory state/outcome
  combinations before Node publication;
- an approval checksum now covers every initial field, enum, option, oneof and
  service method in the M02 project/workspace descriptor. A reviewer had shown
  that the earlier semantic-minimum list did not notice three altered field
  numbers on this still-unmerged schema.
- a second architecture pass found two remaining race/identity edges: adopting
  an already-local portable ID must preserve its existing policy under an exact
  compare-and-set revision, and forking is valid only during registration, not
  while rebinding an existing project;
- terminal operation receipts are immutable when cancellation loses a race to
  completion; the cancellation command becomes an idempotent already-terminal
  no-op rather than rewriting success or failure history.
