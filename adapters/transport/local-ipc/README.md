# Dennett local IPC

This adapter compiles the committed M01 Protobuf contract and carries it over
an authenticated per-install local transport.

- Windows uses a current-user-only Named Pipe and validates both peer process
  identities before gRPC sees the connection.
- The bootstrap proof is short lived, single use and bound to one accepted
  connection, installation and Authority Epoch.
- The renderer receives typed presentation frames, never the pipe name,
  challenge, proof or authenticated client-session identity.
- No TCP listener or localhost fallback is provided.

The server side depends inward on the embedded Head's `SystemStatePort`; the
transport never owns the authoritative project/session projection.
