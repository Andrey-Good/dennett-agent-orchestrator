# Agent Fixture Sets

This directory holds portable-agent JSON fixtures for Stage 3 contract support.

## Layout

| Folder | Meaning |
| --- | --- |
| `valid/` | Contract-shaped files that should validate successfully. |
| `invalid/` | Contract-shaped files that should fail for one targeted reason. |

## Rules

- Keep fixtures machine-oriented and compact.
- Prefer one targeted violation per invalid file.
- Keep filenames descriptive enough that the failure reason is obvious without opening the file.
- Keep constrained-param fixtures portable: validate declarations only and do not imply future UI or runtime auto-migration behavior.
