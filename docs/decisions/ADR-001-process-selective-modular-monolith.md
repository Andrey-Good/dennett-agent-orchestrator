
# ADR-001: Use a process-selective modular monolith

- **Status:** accepted
- **Date:** 2026-07-13

## Context

Dennett needs persistent background behavior, local device capabilities, replaceable providers and crash isolation, but is initially a personal system and should remain installable and understandable by one developer.

## Decision

Use strict modules and ports inside a small number of processes. Split a process only for independent lifecycle, privilege, resource isolation, reuse, language/runtime requirements or measurable fault containment.

## Consequences

The first installation avoids microservice operations while preserving boundaries that can be separated later. Architecture fitness tests enforce dependency direction.
