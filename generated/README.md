# Generated protocol clients

This directory contains committed, reproducible client artifacts generated from
the canonical Protobuf schemas in `protocols/proto`.

- `rust/` is produced by pinned `prost` and `tonic` Buf plugins.
- `ts/` is produced by the pinned `protoc-gen-es` Buf plugin.
- Every generated source carries a `DO NOT EDIT` header.
- Run `just generate` after an approved schema change.
- Run `just test-contracts` to lint and format schemas, compare output byte for
  byte, check compatibility with `main`, and exercise a negative breaking probe.

Do not edit generated sources manually. WP-M00-002 intentionally does not wire
these clients into production crates or packages; their runtime dependencies
belong to the later consumer package that first compiles them.
