# Модуль G. Resource Pressure and Usage Accounting Contract

> **Канонический cross-domain supplement · `G`**  
> **Primary owner:** 50 Server Runtime.  
> **Происхождение:** выделено из предархитектурного gap-аудита. Документ актуален и нормативен.  
> Маркеры источников вида `[[Sxx]]` раскрыты в [`REFERENCES.md`](REFERENCES.md). Ownership и порядок чтения описаны в [`README.md`](README.md).


## G.1. Назначение

Denet постоянно использует ограниченные ресурсы:

- дисковое пространство;
- RAM/VRAM;
- CPU/GPU/NPU;
- battery и thermal budget;
- network traffic;
- provider token/API cost;
- subscription quotas;
- rate limits;
- background execution time;
- пользовательское внимание.

Ресурсная логика не должна превращаться в корпоративный billing-service, но система обязана:

- не терять данные молча;
- не разряжать телефон незаметно;
- не сжигать provider limits на низкоприоритетный фон;
- объяснять стоимость проекта/Run;
- адаптировать sensory capture;
- сохранять interactive responsiveness;
- позволять пользователю задавать бюджеты.

## G.2. Главная модель

> Denet измеряет ресурсы в их нативных единицах, нормализует их для сравнения и связывает с Project, Task, Run, Capability, Device и Source. Точная стоимость, оценка и неизвестность не смешиваются.

FOCUS предлагает vendor-neutral normalization billing data across AI, cloud, SaaS and data center; Denet использует тот же принцип для provider cost, но расширяет его local compute, storage и battery. [[S38]]

## G.3. Resource Dimensions

### Monetary

- provider-reported API cost;
- estimated subscription consumption;
- cloud compute/storage/network;
- paid plugin/service.

### Compute

- CPU time/load;
- GPU time/utilization;
- NPU/accelerator;
- RAM/VRAM residency;
- local model load/eviction.

### Storage

- canonical events;
- evidence/media;
- artifacts;
- projects;
- backups;
- indexes/cache;
- temporary worktrees/models.

### Network

- upload/download;
- metered/mobile;
- peer sync;
- cloud media;
- provider streaming.

### Device

- battery;
- thermal pressure;
- foreground/background restrictions;
- microphone/camera/screen active duration.

### Attention

- notifications;
- approval prompts;
- voice interruptions;
- review workload.

## G.4. Usage Observation

```yaml
usage_observation:
  observation_id: id
  source: provider | runtime | device | estimate
  subject_refs: []
  resource_dimension: typed
  quantity: number
  unit: canonical_unit
  monetary_value: optional
  currency: optional
  confidence: exact | provider_reported | estimated | unknown
  interval: start_end
  observed_at: time
  attribution_quality: typed
```

Provider invoice/report remains authority for billable cost. Denet estimates are explicitly marked.

## G.5. Attribution

Usage can attach to:

- installation;
- user;
- project;
- session;
- Task/Run;
- agent/provider session;
- capability/tool;
- ambient source;
- device;
- maintenance job.

Shared overhead may be:

- attributed proportionally;
- kept as system overhead;
- not falsely assigned to one project.

## G.6. Budgets

```yaml
resource_budget:
  budget_id: id
  scope_ref: ref
  dimensions: {}
  soft_limits: {}
  hard_limits: {}
  reset_period: optional
  priority: typed
  exceed_policy: warn | degrade | pause | ask | stop
  owner_ref: ref
```

Budgets may be:

- global monthly API;
- per-project token/time;
- per-Run;
- ambient battery/network;
- storage reserve;
- local GPU concurrency;
- provider subscription reserve.

## G.7. Soft и hard limits

### Soft

- warning;
- cheaper/local backend suggestion;
- reduced helper agents;
- defer maintenance;
- lower capture quality;
- summary instead of full report.

### Hard

- stop/pause according to completion safety;
- never interrupt mid external effect without reconciliation;
- preserve checkpoint and partial artifact;
- user can grant bounded extension.

## G.8. Storage pressure policy

States:

```text
NORMAL
→ WATCH
→ PRESSURE
→ CRITICAL
→ EMERGENCY_READ_ONLY
```

Thresholds depend on absolute free space, percentage, growth rate and reserved recovery space.

### G.8.1. Reclamation order

1. disposable UI/cache/temp;
2. regenerable previews/thumbnails;
3. stale downloaded model cache if recoverable;
4. rebuildable indexes with rebuild plan;
5. duplicate raw media after integrity/dedup check;
6. expired ring buffers/candidates;
7. cold media according to retention/offload policy;
8. user-visible decision for canonical/valuable data.

Нельзя автоматически удалять:

- canonical Memory Events;
- only copy of artifact;
- unsynced project data;
- recovery keys;
- pending effect receipts;
- evidence under legal/user retention.

### G.8.2. Sensory degradation

При pressure:

- lower screenshot frequency/resolution;
- prefer accessibility/DOM metadata;
- shorten raw audio retention;
- commit transcript/structured event and discard low-value raw where policy allows;
- pause nonessential source;
- offload cold encrypted media;
- notify without spam.

Система показывает конкретно, что изменилось.

### G.8.3. Canonical append failure

Если durable append не гарантирован:

- source enters degraded/paused;
- no false indication that capture continues;
- local emergency buffer bounded;
- user notified if data may be lost;
- never drop silently.

## G.9. Battery and thermal policy

Mobile profiles:

- charging/unmetered;
- normal battery;
- low power;
- thermal pressure;
- user active/navigation/call.

Adaptive actions:

- local VAD remains, heavy ASR deferred;
- reduce screen/audio semantic analysis;
- stop preloading large models;
- sync metadata first, media later;
- use server node;
- keep emergency commands local.

## G.10. Network policy

- direct/peer transfer preferred for large local objects where safe;
- metered network can defer media/backups/models;
- control/permissions/cancel retain priority;
- user can force sync/download;
- background upload resumable;
- no repeated full upload after reconnect if chunk/content IDs exist.

## G.11. Provider quota policy

For each provider:

- known remaining quota if exposed;
- rate limit state;
- billing unit;
- subscription estimate;
- reset time;
- priority reserve;
- fallback policy.

Unknown subscription consumption remains estimate; Denet does not invent exact remaining messages.

Interactive user chat may reserve provider capacity over background work.

## G.12. Cost-aware agent execution

Before spawning helper/reviewer/deep mode:

- estimate marginal utility;
- estimate token/time/provider impact;
- respect execution profile;
- reuse existing context/results;
- avoid duplicate research;
- stop no-progress loops.

User sees budget consequences in plain language, not raw token count only.

## G.13. Attention budget

Resource accounting includes:

- prompts per task;
- voice interruptions;
- Inbox backlog;
- notification frequency;
- review minutes estimated/observed.

If Denet repeatedly asks same low-risk decision, it proposes bounded policy rather than continuing to consume attention.

## G.14. Resource Coordinator

Logical function, not necessarily a service or agent.

Responsibilities:

- collect usage observations;
- maintain budget state;
- emit pressure events;
- provide eligibility constraints to Capability/Agentic;
- execute deterministic degradation policy;
- request user only when trade-off meaningful.

No LLM required for ordinary thresholds.

## G.15. Resource-aware scheduler

Priority order remains:

1. stop/cancel/permission/voice;
2. user waiting;
3. active project;
4. background;
5. maintenance.

Under pressure:

- maintenance and speculative tasks yield first;
- checkpoint long tasks;
- avoid simultaneous local models exceeding VRAM;
- protect Head/runtime health.

## G.16. Usage history and forecast

Denet may show:

- daily/monthly trend;
- project breakdown;
- provider/model breakdown;
- local vs cloud;
- ambient cost;
- projected exhaustion.

Forecast is marked estimated and should use simple statistical projection before model-generated narrative.

## G.17. Failure and recovery

### Provider reports delayed cost

Backfill usage and update estimates; never rewrite historical estimate as if it was exact without provenance.

### Metric missing

Mark unknown; do not assume zero.

### Counter reset/provider change

Segment observation by source version/account.

### Device offline

Local counters sync later with IDs/intervals and dedup.

### Disk fills during migration

Migration aborts safely, rollback/checkpoint; reserved recovery space protected.

## G.18. Observability

OpenTelemetry semantic conventions provide common naming across traces, metrics and logs, including GenAI, hardware, devices and system resources. Denet should align where practical, while keeping product usage records separate from raw telemetry. [[S39]]

## G.19. Evaluation

- billing error vs provider invoice;
- unknown usage rate;
- project attribution coverage;
- budget enforcement correctness;
- interactive latency under background load;
- storage pressure recovery;
- battery impact ambient modes;
- unnecessary agent cost;
- resource-related user prompts;
- data loss under disk full tests.

## G.20. Антиоверинижиниринговые ограничения

Не создавать:

- accounting ledger уровня банка;
- exact subscription prediction where provider hides data;
- LLM cost reviewer per call;
- one resource microservice per dimension;
- arbitrary automatic deletion to hit budget;
- optimization that hides quality/provider changes.

## G.21. Критерии готовности

- resource dimensions explicit;
- exact vs estimated separated;
- soft/hard budget behavior defined;
- storage reclamation order safe;
- ambient degradation visible;
- canonical data protected;
- attention included;
- interactive work prioritized;
- offline usage deduplicates.

## G.22. Карта будущего переноса

- `50 Server`: coordinator, scheduler, pressure handling.
- `41 Capability`: provider quota/model/local hardware measurements.
- `20 Agentic`: marginal cost and budget behavior.
- `10 Memory`: retention/tiering/rebuildable data.
- `40 Voice`: ambient resource adaptation.
- `60/61 UI`: usage dashboards/warnings/settings.

---
