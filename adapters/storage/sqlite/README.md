# SQLite control store

Implements provider-neutral project-session and non-canonical draft-cache ports for the local-only embedded Head profile.

The adapter uses one writer connection, WAL, a bounded busy timeout, versioned SQLx migrations and per-event SHA-256 integrity checks. Startup refuses future, failed or corrupt schema/history instead of deleting or reinterpreting user data. `session_events` is canonical for this local profile; `client_drafts` is explicitly non-canonical even though both tables live in the same `control.sqlite` file.
