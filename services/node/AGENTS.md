
# Denet Device Node

Persistent local daemon. Owns local capabilities, offline queue, device IPC, project workspace access, local models and sensor coordination.

- UI is a client; closing it must not stop the Node.
- Do not claim global Head authority unless the user previously granted eligibility and the Head promotion contract passes.
- Client SQLite is cache/offline state, not a second canonical memory.
- Local consequential commands are queued/revalidated unless an explicit offline policy permits them.
