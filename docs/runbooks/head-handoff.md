# Head Handoff

1. Confirm the target device has `head_eligibility = full`.
2. Verify strong owner authentication and recovery-key availability.
3. Verify canonical memory/object-store completeness and sync watermarks.
4. Drain new consequential effects on the current Head.
5. Record outstanding/unknown effects.
6. Increment Authority Epoch and issue fencing token.
7. Start target Head, run readiness and semantic smoke tests.
8. Redirect Nodes and confirm old Head rejects new writes.
9. Preserve rollback window; never run both Heads without fencing/witness guarantees.
