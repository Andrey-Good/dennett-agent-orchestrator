
# Mobile Application Instructions

Implement the interruption-safe trusted remote from specification 61 and architecture volume 83.

- React Native owns shared presentation; Kotlin/Swift native modules own pairing, durable queue, background work, notifications, voice and OS surfaces.
- A notification/widget action is a canonical idempotent command, not local fake state.
- Persist draft/capture before organization.
- Expose stale/offline freshness and provide `Continue on Desktop` for unsuitable work.
- Never store master secrets in JavaScript storage.
- Test process death, background limits, one-handed use, accessibility and reconciliation.
