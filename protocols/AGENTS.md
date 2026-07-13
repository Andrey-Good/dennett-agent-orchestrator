
# Protocol Instructions

- Protobuf messages are wire DTOs, never domain models.
- Never reuse field numbers; reserve removed fields.
- Run Buf lint/format/generate/breaking checks after changes.
- Additive fields need explicit default/absence semantics.
- A semantic breaking change requires a new message/field and migration plan.
- Do not place secrets or raw provider-specific payloads in common envelopes.
