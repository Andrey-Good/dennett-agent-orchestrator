# dennett-observability

This crate owns the local Personal Quiet diagnostic boundary. It is operational
evidence, not canonical user memory, security audit or product analytics.

## Current profile

- a per-user default root (`%LOCALAPPDATA%\Dennett\data` on Windows,
  `~/Library/Application Support/Dennett/data` on macOS and the XDG data root
  on Linux) rather than a shared temporary directory;
- handle-relative, no-follow filesystem access for every managed child, with
  current-user-only ACLs on Windows and `0700` directories / `0600` files on
  Unix;
- structured JSONL logs under `<DENNETT_DATA_DIR>/diagnostics/logs`;
- daily and size-based rotation with at most 14 log files, 14 days and 32 MiB
  per component; a long-lived Node reclaims old files instead of treating the
  byte bound as a lifetime quota;
- a lossy non-blocking writer, so a slow or full diagnostic disk cannot stall
  Node; queue, capacity and physical-write losses are retained in lifecycle
  evidence and `doctor`, including after an abnormal exit;
- at most 64 terminal lifecycle records per component;
- an independently locked active-run marker, readable by `doctor` while the
  process is alive;
- restart reconciliation that distinguishes clean, handled failure and
  unclean previous exit and preserves the last durable safe phase;
- monotonic per-component run sequencing, so a wall-clock rollback cannot make
  an older exit look newer;
- bounded reads, directory enumeration and lock acquisition while inspecting
  or maintaining diagnostics;
- a five-second caller-side shutdown deadline. A stalled diagnostic disk can
  leave a recoverable active marker, but it cannot hold the application open
  indefinitely;
- one UUID run identifier on every record plus UUID-only
  project/session/command/runtime-turn references;
- a provider allowlist that maps unknown adapter text to `other`;
- fixed, privacy-safe adapter-host failure classifications for stdout protocol
  failures and a strict structured stderr channel that never copies raw stderr.

Only `DiagnosticEvent` emitted by this crate enters persistent logs. Its
message and classification are static and its references accept UUID values,
not provider-supplied text. Ordinary `tracing` events, including an event that
merely spoofs the private diagnostic target, are excluded from the Personal
Quiet writer. This prevents a mistaken prompt, response, token, path or
arbitrary field from becoming durable diagnostic data.

If the diagnostic directory cannot be initialized, the Device Node reports a
generic safe code and continues with console-only tracing. Canonical project
state does not depend on these files.

## Inspection

```text
dennettctl doctor --data-dir <profile-path>
dennettctl doctor --data-dir <profile-path> --json
```

The text summary reports storage health, bounded log volume, dropped records,
live/stale/unreadable marker counts, the latest terminal exit code and its last
durable phase. It also says whether the previous shutdown flush and final drop
count were confirmed. A newer corrupt terminal record produces `unknown`
rather than allowing an older clean exit to masquerade as current. Wrong-type
directories are reported as an invalid layout instead of an empty healthy
profile. The text form deliberately hides the absolute profile path; explicit
JSON output retains the path for local tooling. Neither form inspects private
project files or internal database tables.

The profile cannot invent a root cause that the operating system or provider
did not expose. In that case it preserves the last durable phase and an honest
generic error classification. Support-bundle export and a desktop Diagnostics
workspace remain later work.
