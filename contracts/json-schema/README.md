# JSON Schema Contracts

This package contains the portable JSON Schema 2020-12 contracts used by the orchestrator.

## What the schemas enforce directly

- Required properties, types, enums, `const`, `additionalProperties: false`, and array cardinality.
- Per-item object structure for binding lists, node lists, and request payloads.
- Simple intra-object dependencies that standard JSON Schema can express locally.

## What is delegated to the invariant/test layer

These rules are intentionally not faked in schema:

- uniqueness of an object field across array items, such as binding `id` values in `skills`, `mcps`, `plugins`, `memory_bindings`, `runtime_sources`, and `nodes`;
- uniqueness of `options[].id` in the built-in MCP request schema;
- inequality between sibling marker strings such as `secret_markers.open_marker` and `secret_markers.close_marker`;
- deep-value uniqueness of `params.<name>.allowed_values`;
- semantic compatibility between `params.<name>.type`, `params.<name>.constraints`, and the values used in `default` or `allowed_values`.

The schema files may include `$comment` notes at those locations to make the boundary explicit, but the actual enforcement belongs in invariants and tests.
