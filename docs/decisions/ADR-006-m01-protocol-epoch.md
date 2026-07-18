# ADR-006: M01 replaces the non-production protocol scaffold as one epoch

**Status:** Proposed; owner approval is required before merge

**Date:** 2026-07-18

**Work Package:** `WP-M01-002`

## Context

M00 intentionally generated all wire types under `dennett.v1` and retained ten
Buf STANDARD exceptions. `DEBT-0001` and `DEBT-0002` require that scaffold to be
replaced before the first production or external client compiles against it.
No production consumer exists, so compatibility shims would preserve the wrong
public shape without protecting user data or a deployed client.

M01 needs only the local desktop conversation boundary: authenticated handshake,
bootstrap and health, typed project/session commands, turn cancellation, safe
errors, and revisioned system/session snapshot-delta streams.

## Decision

Perform one atomic pre-production protocol epoch:

- retire package `dennett.v1` and every generated symbol from the M00 scaffold;
- introduce `dennett.common.v1`, `dennett.control.v1`, and `dennett.sync.v1`;
- use typed `SystemService`, `ProjectService`, and `SessionService` RPCs;
- acknowledge command admission separately from authoritative completion;
- return only allocated stable identity with create admission; publish completion
  as a typed command terminal plus state mutation through the authoritative
  system watch and subsequent query;
- deliver system and session state as an initial snapshot followed by monotonic
  deltas;
- include the authenticated Authority Epoch in every watch cursor;
- stop delta application on sequence, revision, or epoch mismatch and require a
  typed resync reason plus fresh handshake/bootstrap before accepting a
  different epoch;
- keep provider payloads, credentials, hidden reasoning, database records, memory,
  voice, mobile, and external-effect contracts outside this epoch;
- require Buf STANDARD lint with no ignores;
- generate Rust and TypeScript artifacts only from canonical Protobuf sources.

Every unary response has an explicit result-or-error `oneof`. Domain failures
occupy the typed `dennett.common.v1.ErrorEnvelope` arm; a non-OK gRPC status is
reserved for transport/protocol failure that prevents delivery of a decodable
Dennett response. This avoids language-specific status-detail conventions and
`google.protobuf.Any`. Watch failures use the same envelope inside the typed
system or session frame. A frame-level error is a stream failure; domain command
and turn failures use `CommandTerminal.error` and `TurnTerminal.error`.

The handshake returns a short-lived proof and opaque `client_session_id` to the
trusted Tauri/Node bridge. Bootstrap consumes both; later commands, authenticated
queries, and watches carry the session ID, which remains bound to the validated
OS peer, installation and Authority Epoch. Neither value reaches the React
renderer or logs. Reconnect or channel replacement repeats handshake/bootstrap;
client sessions cannot be transferred between peers or pooled connections.

An accepted create command returns only its allocated project/session ID. That
identity is not proof of completed domain mutation. `SystemService.Watch`
delivers the correlated `CommandTerminal` success/error and the authoritative
summary mutation in one ordered revision transition; full project state is then
read through `GetProject`. Turn completion remains authoritative through
`SessionService.WatchSession`.

## Compatibility and migration evidence

[`protocols/epoch-migrations/m00-to-m01.json`](../../protocols/epoch-migrations/m00-to-m01.json)
records the exact old/new protocol-tree fingerprints, retired and introduced
packages, generated public symbol families, decision reference, and owner gate.

The compatibility checker first runs normal `buf breaking`. A failure is accepted
only when both protocol trees and the complete package transition exactly match
that hash-pinned manifest. After this epoch reaches `main`, ordinary additive
`WIRE_JSON` compatibility resumes; the historical exception cannot authorize a
different future break.

There is no persisted-data migration. The only migration is compile-time removal
of non-production generated APIs. The old and new generated packages must never
coexist in one build.

## Consequences

- Desktop, Node, and runtime work can compile against stable subsystem packages.
- Strict lint debt is removed instead of hidden behind compatibility aliases.
- M00 generated imports and full RPC names intentionally stop compiling.
- Memory, voice, object transfer, mobile, and external APIs will be introduced by
  their owning work packages without speculative placeholders here.
- Any owner rejection rolls back the complete epoch, restores both debt records,
  and leaves no mixed generated surface.

## Owner gate

Before merge, the owner must accept the breaking pre-production epoch and the
bounded M01 wire surface. Until then `WP-M01-002` may reach review but not
`MERGE_READY` or `MERGED`.
