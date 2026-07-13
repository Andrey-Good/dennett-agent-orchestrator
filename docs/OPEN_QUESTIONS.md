# Open Implementation Questions

These are intentional architecture choices that remain to be resolved by ADRs or risk spikes. Coding agents must not silently make them global defaults.

| Question | Current baseline | Decision gate |
|---|---|---|
| SQLite encryption | SQLCipher candidate | packaging, performance and recovery spike |
| Local vector index | adapter boundary; no final backend | realistic retrieval benchmark on target devices |
| Dedicated vector service | PostgreSQL/pgvector first | filtered ANN, rebuild and latency benchmark |
| Self-hosted object store | filesystem/S3 port | personal-server scale and operational-cost test |
| Durable workflow engine | lightweight Managed Run first | only if timers/replay/retries reproduce Temporal/Restate complexity |
| Device transport | direct TLS plus optional Tailscale/Headscale | NAT, mobile and failure-spike results |
| Mobile transport | generated gRPC baseline | switch to HTTPS/WebSocket only on measured platform friction |
| React Native vs native mobile | React Native presentation + native node | replace if OS integrations or stability fail acceptance gates |
| Screen capture backend | Screenpipe candidate + native fallback | license, privacy, resource and fidelity spike |
| Computer-use backend set | structured-first resolver | per-backend reliability and safety benchmarks |
| Realtime voice transport | one chained + one realtime backend | latency, interruption and strong-sidecar spike |
| Exact provider set for first public build | fake + 1–2 high-value runtimes | maintenance value and user demand |
| RPO/RTO defaults | conservative personal-server proposal | measured backup/restore drill |
| Default sensory retention | profile-based, local-first | storage, privacy and user-study results |
| Public license | all rights reserved currently | owner decision before public release/contributions |
| JavaScript lockfile | generate in first dependency-resolution commit | successful clean install and CI pass |

Architecture volumes contain the full alternatives and replacement triggers. Add an ADR when a decision becomes binding.
