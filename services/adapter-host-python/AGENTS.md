
# Python Adapter Host

Runs external Python SDK integrations out of process. It is not a domain owner.

- Use versioned JSON/protobuf messages.
- Support describe, health, invoke, cancel, drain and shutdown.
- Enforce deadlines and bounded output.
- Never receive broad owner credentials when a scoped brokered operation is possible.
- A malformed adapter must fail only its invocation/host, not Head.
