# Subagent Reviewer Checklist Template

Use this checklist for reviewer subagents assigned to substantial implementation or documentation tasks.

## Review Target

- Task document:
- Worker:
- Reviewer:
- Date:
- Claimed changed files:
- Allowed write scope:

## Scope Control

- Changed files stay inside allowed scope:
- No forbidden files were edited:
- Existing user or parallel-agent work was not reverted:
- Generated files were not committed unless explicitly allowed:

## Correctness

- Implementation satisfies the task goal:
- Edge cases and negative cases are handled:
- Error states fail honestly:
- No behavior is claimed without evidence:

## Architecture

- Existing owner boundaries are preserved:
- Runtime/provider-specific behavior stays behind adapters:
- Storage, interface, lifecycle, and core responsibilities are not mixed:
- Reused primitives are preferred over custom replacement logic:
- Any boundary exception is documented:

## Tests And Validation

- Required tests were added or updated:
- Existing relevant tests were run:
- Validation output is recorded:
- Missing validation is justified:

## Documentation And Evidence

- Docs, code, tests, examples, and live proof are distinguished:
- Invented logic is captured in a durable place:
- User-facing claims match actual behavior:
- No hidden shortcut is required for the change to work:

## Findings

| Priority | Finding | File/location | Required fix |
| --- | --- | --- | --- |
|  |  |  |  |

Priorities: P0 blocking correctness/safety, P1 serious regression or architecture risk, P2 important quality gap, P3 minor cleanup.

## Decision

- Decision: approve / request fixes / block
- Required fix owner:
- Re-review required: yes/no
- Reviewer rationale:
