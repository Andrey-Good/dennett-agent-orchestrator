[English](#english) | [Russian](#russian)

<a id="english"></a>
# Live Proof Runbook

Status: runbook template. A completed run proves only the target, environment, provider accounts, runtime versions, and dates recorded in the evidence log.

## Purpose

Use this runbook to prove selected end-to-end product flows against real external dependencies. Local adapters, fake providers, mocks, and dry runs may support preparation, but they do not satisfy the live proof requirement.

## Preconditions

- Release scope and included capabilities are named.
- Real runtime and provider accounts are available with limits and regions recorded.
- Secrets are configured through approved local or deployment mechanisms and are not copied into logs.
- Test users, projects, agents, and memory/provider resources are disposable or explicitly approved.
- Observability is enabled for commands, run IDs, provider request IDs, error classes, and final outputs.
- Rollback or cleanup steps are understood before the run starts.

## Required Scenarios

Record each scenario in [Evidence Log](./evidence-log.md):

| Scenario ID | Scenario | Minimum Proof |
| --- | --- | --- |
| `P19-LIVE-CLI-001` | CLI starts and completes a realistic graph using a real runtime. | Final output, run state, runtime request IDs, and redacted transcript. |
| `P19-LIVE-MEM-001` | A flow writes, reads, and searches external memory through the internal memory layer when memory is in release scope. | Provider object IDs or redacted query traces, retrieved result, and cleanup result. |
| `P19-LIVE-CHAT-001` | User interaction blocks, records a reply, resumes, and preserves final state when user chat is in release scope. | Prompt state, reply state, resume state, and final output. |
| `P19-LIVE-SUB-001` | Managed subagent orchestration creates, waits, reviews, and closes governed child work when managed subagents are in release scope. | Parent run ID, child lineage, role/status transitions, and closure evidence. |
| `P19-LIVE-BUILD-001` | Builder-authored output is validated, published through lifecycle, and executed when builder release scope is included. | Draft identity, validation result, deploy/live identity, and execution result. |

If a scenario is outside the release scope, mark it `not-run` with `decision_effect: supports-defer` and link the release-scope decision. If it is inside scope and cannot run, mark it `blocked`.

## OSS v0.1 Provider Proof Matrix

Use this matrix before any OSS public-launch provider claim expands beyond the current local/package boundary. It separates local/offline checks, user-owned Mem0 flows, Codex/App Server live flows, and unsupported claims.

| Claim class | Scenario IDs | Current claim status | Required proof before claim expands |
| --- | --- | --- | --- |
| Local/offline CLI and graph semantics | `P19-LIVE-LOCAL-001` | Evidence exists only through repository gates, local examples, deterministic tests, and local package proof. | Run the documented local gates for the exact candidate artifact and record commit, package version, OS, Node.js, command list, and final result. |
| User-owned Mem0 direct provider path | `P19-LIVE-MEM0-DIRECT-001` | Narrow local Mem0 registration, read, write, search, and scoped namespace cleanup have historical evidence. | For OSS v0.1 claims, rerun against a disposable user-owned provider namespace or local SDK store, record registration config class, operation IDs, cleanup result, and redactions. |
| Codex plus registered Mem0 runtime-memory path | `P19-LIVE-MEM0-CODEX-001` | Historical evidence proves only prompt-rendered memory context plus success-only Core writes. | Rerun the runtime-memory fixture against the candidate artifact, prove memory-influenced output and post-success write metadata, and keep native App Server memory explicitly out of scope unless separately proven. |
| Codex/App Server live runtime graph | `P19-LIVE-CODEX-CLI-001` | Historical narrow live smoke exists for a minimal graph, not broad model/options support. | Rerun a quota-safe graph against the exact candidate artifact, record App Server version or capability metadata, model/options used, final output, run ID, and redacted account evidence. |
| Runtime discovery and account/config introspection | `P19-LIVE-CODEX-DISCOVERY-001` | Historical local authenticated discovery exists. | Rerun `runtime-env-inspect --redacted` and model discovery against the candidate artifact; record only redacted metadata and unsupported/unknown fields. |
| External provider reliability, throttling, latency, or volume | `P19-LIVE-PROVIDER-RELIABILITY-001` | Not proven for public claims. Deterministic local stub tests are not live reliability proof. | Run quota-safe live-provider reliability scenarios with named limits, retry behavior, failure handling, cleanup debt, and residual risk. Mark `blocked` if accounts, quota, or safe cleanup are unavailable. |
| Native App Server memory, non-Codex runtimes, hosted provider operations, provider-wide cleanup | `P19-LIVE-UNSUPPORTED-001` | Unsupported or unproven. | Add implementation and live evidence first, or keep these claims forbidden/deferred in launch docs. |

## Provider Proof Evidence Schema

Use this schema for each provider proof row before adding it to the evidence log. A blocked row is acceptable evidence of missing prerequisites; it is not a successful provider proof.

```yaml
id: P19-YYYY-MM-DD-PROVIDER-...
type: live-proof | regression | manual-review
scenario_id: P19-LIVE-CODEX-CLI-001
claim_class: local-offline | mem0-direct | mem0-codex | codex-live | provider-reliability | unsupported
target_claim: ""
environment:
  os: ""
  node: ""
  package_manager: ""
  runtime_or_provider: ""
  account_or_namespace_class: redacted | disposable | not-used
artifact_or_commit: ""
commands_or_procedure: ""
result: pass | fail | blocked | inconclusive | not-run
decision_effect: supports-release | blocks-release | supports-defer | informational
proof_observed:
  final_output: ""
  persisted_state: ""
  provider_objects: ""
  cleanup_or_retention: ""
redactions:
  - ""
claim_boundary: ""
missing_prerequisites:
  - ""
review_status: unreviewed | accepted | rejected | superseded
```

## Execution Steps

1. Record environment metadata before the run.
2. Execute one scenario at a time with fresh run IDs.
3. Capture logs and artifacts immediately after each scenario.
4. Redact secrets, account identifiers, user PII, and provider-specific private data.
5. Verify persisted state and cleanup behavior.
6. Record the result as `pass`, `fail`, `blocked`, or `inconclusive`.
7. Stop the release proof if a failure can corrupt shared provider state or invalidate later evidence.

## Pass Rules

A live proof scenario passes only when:

- the full user-visible flow completes against real dependencies;
- captured artifacts show the same run identity across command, state, provider, and final output;
- cleanup or retention behavior is known and documented;
- no unhandled exception, silent data loss, duplicate finalization, or hidden manual repair was required;
- residual risk is stated.

## Failure Handling

Do not rerun until the failed attempt is logged. A retry may be added as a separate evidence item, but it must not replace the failed record.

<a id="russian"></a>
# Runbook для live proof

Статус: шаблон runbook. Завершенный запуск доказывает только target, environment, provider accounts, runtime versions и dates, записанные в evidence log.

## Назначение

Используйте этот runbook, чтобы доказать выбранные end-to-end product flows с реальными внешними зависимостями. Local adapters, fake providers, mocks и dry runs могут помогать подготовке, но они не удовлетворяют требованию live proof.

## Предусловия

- Область выпуска и включенные capabilities названы.
- Доступны реальные runtime и provider accounts, а limits и regions записаны.
- Secrets настроены через одобренные локальные или deployment механизмы и не копируются в logs.
- Test users, projects, agents и memory/provider resources являются одноразовыми или явно одобрены.
- Observability включена для commands, run IDs, provider request IDs, error classes и final outputs.
- Rollback или cleanup steps понятны до начала запуска.

## Обязательные сценарии

Записывайте каждый сценарий в [Evidence Log](./evidence-log.md):

| Scenario ID | Scenario | Minimum Proof |
| --- | --- | --- |
| `P19-LIVE-CLI-001` | CLI запускает и завершает реалистичный graph с реальным runtime. | Final output, run state, runtime request IDs и отредактированный transcript. |
| `P19-LIVE-MEM-001` | Flow пишет, читает и ищет во внешней memory через internal memory layer, если memory входит в release scope. | Provider object IDs или отредактированные query traces, retrieved result и cleanup result. |
| `P19-LIVE-CHAT-001` | User interaction блокируется, записывает reply, resumes и сохраняет final state, если user chat входит в release scope. | Prompt state, reply state, resume state и final output. |
| `P19-LIVE-SUB-001` | Managed subagent orchestration создает, ждет, reviews и закрывает governed child work, если managed subagents входят в release scope. | Parent run ID, child lineage, role/status transitions и closure evidence. |
| `P19-LIVE-BUILD-001` | Builder-authored output валидируется, публикуется через lifecycle и выполняется, если builder release scope включен. | Draft identity, validation result, deploy/live identity и execution result. |

Если сценарий вне release scope, отметьте его `not-run` с `decision_effect: supports-defer` и ссылкой на решение об области выпуска. Если он внутри scope и не может быть выполнен, отметьте его `blocked`.

## Шаги выполнения

1. Запишите environment metadata до запуска.
2. Выполняйте по одному сценарию за раз со свежими run IDs.
3. Захватывайте logs и artifacts сразу после каждого сценария.
4. Редактируйте secrets, account identifiers, user PII и provider-specific private data.
5. Проверяйте persisted state и cleanup behavior.
6. Записывайте результат как `pass`, `fail`, `blocked` или `inconclusive`.
7. Остановите release proof, если failure может испортить shared provider state или сделать последующие доказательства недействительными.

## Правила pass

Live proof scenario проходит только когда:

- полный user-visible flow завершается с реальными зависимостями;
- captured artifacts показывают один и тот же run identity в command, state, provider и final output;
- cleanup или retention behavior известны и задокументированы;
- не потребовались unhandled exception, silent data loss, duplicate finalization или hidden manual repair;
- residual risk указан.

## Обработка failure

Не запускайте повторно, пока failed attempt не записан. Retry может быть добавлен как отдельный evidence item, но он не должен заменять failed record.
