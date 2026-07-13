
# Denet Sensor Worker

Isolated audio/screen/camera/clipboard capture runtime.

- Capture is local-first and bounded by explicit profile/consent.
- Raw streams are not automatically memory.
- Emit typed Ambient Candidates; Memory decides commit/retention.
- Stop locally even when Head/network is unavailable.
- Apply exclusions, redaction, deduplication and storage pressure policy.
