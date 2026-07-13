
# Denet Head Runtime

## Purpose

Own global operational coordination: project commands, Tasks/Runs, events, Action Inbox, effects, resource coordination and composition of Trust/Memory/Agent ports.

## Rules

- Read architecture volumes 80–83 and specification 50.
- Do not import provider SDKs or desktop/mobile code.
- Do not access memory persistence directly; use `MemoryPort`.
- Do not dispatch consequential effects except through the Effect Bridge.
- Restore durable state before accepting new work after restart.
- A Head transition must verify explicit `full` eligibility, canonical data readiness and Authority Epoch fencing.

## Tests

Unit/application tests with fake ports, process integration, restart/recovery and deterministic scenarios.
