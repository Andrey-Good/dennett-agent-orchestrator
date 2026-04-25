[English](#english) | [Русский](#russian)

<a id="english"></a>
# Validation Matrix

Status: normative.  
Owns: the current-stage validation matrix for release-facing confidence.  
Does not own: subsystem semantics or the exact implementation of each test.  
Primary sources: [release gates](./release-gates.md), [contracts](../03-contracts/README.md), [execution](../04-execution/README.md), [state](../05-state/README.md), [lifecycle](../07-lifecycle/README.md), [extensions](../08-extensions/README.md), [Phase 19 real-world proof and release](../20-real-world-proof-and-release/README.md).

## 1. Purpose

This matrix makes the Phase 11 validation surface explicit. It prevents quality work from turning into an untracked mix of scripts, reviewer intuition, and remembered edge cases.

## 2. Validation Classes

| Validation area | What must be protected | Current expected validation mode | Notes |
| --- | --- | --- | --- |
| Toolchain integrity | TypeScript compile health, lint compliance, test runner health, build health | Automated in canonical commands and CI | Owned operationally by [release gates](./release-gates.md) |
| Git candidate hygiene | Release evidence comes from tracked or staged product paths, not untracked local work; forbidden local/generated artifacts stay out of the taggable candidate | Automated through `pnpm release-candidate:check` before release staging/sign-off | `dist/`, `.local/`, `subagent_tasks/`, package archives, DB/log/temp artifacts, and stale `contracts/typescript/*.js` must not become tracked candidate content |
| Portable contract validity | Accepted agent-file structure, parameter restrictions, output schema rules, MCP payload shape | Automated | Primary owners live in [contracts](../03-contracts/README.md) |
| Execution correctness | Sequential graph execution, outcomes, final output, direct child-run behavior | Automated | Primary owners live in [execution](../04-execution/README.md) |
| Resume and interruption boundaries | Explicit resume rules, interruption handling, blocked prompt persistence, revision pinning | Automated plus focused manual review when semantics changed | Primary owners live in [state](../05-state/README.md) and [interaction](../06-interaction/README.md) |
| Crash and durability behavior | Atomic writes, metadata ordering, recovery invariants, durable local state | Automated where practical plus focused manual review | Primary owners live in [state](../05-state/README.md) |
| Lifecycle correctness | Registry behavior, drafts, live resolution, deploy semantics, trigger/event persistence | Automated | Primary owners live in [lifecycle](../07-lifecycle/README.md) |
| Extension correctness | Builder draft-only behavior, runtime-source gating, memory-binding honesty, subagent rules already implemented | Automated | Primary owners live in [extensions](../08-extensions/README.md) and [subagent docs](../02-architecture/subagent-orchestration-model.md) |
| Integrated product-flow coherence | Cross-subsystem flows that combine lifecycle, builder output, runtime features, interaction, memory, and managed subagents must keep each subsystem boundary honest | Automated local/offline integration coverage now exists through `tests/integration/phase18-integrated-product-flows.test.ts`; live external, stress, and release-readiness validation remain Phase 19 evidence | Primary owner lives in [Phase 18 Integrated Product Flows](../19-integrated-product-flows/README.md); this class is below, and does not replace, Phase 19 external proof, stress proof, or release-readiness evidence |
| Real-world proof and release decision | Live runtime/provider proof, stress/regression evidence, operational runbooks, and a release decision record must exist before release-readiness claims | Phase 19 evidence-driven validation; local/offline Phase 18 evidence and green repository gates are necessary but not sufficient for release claims | Primary owner lives in [Phase 19 Real-World Proof And Release](../20-real-world-proof-and-release/README.md); a completed [release decision record](../20-real-world-proof-and-release/release-decision-record.md) is required before claiming release readiness |
| Adapter honesty | Codex App Server adapter must not claim unsupported behavior; current capability gates must stay truthful | Automated plus focused manual review | Primary owners live in [runtime integration model](../02-architecture/runtime-integration-model.md) and [runtime adapter contract](../03-contracts/runtime-adapter-contract.md) |
| CLI workflow stability | Accepted commands, arguments, and release-facing workflows remain coherent | Automated plus focused manual review for changed UX | Interface rules depend on existing accepted behavior rather than new Phase 11 syntax |
| Documentation consistency | Owner-doc links, no material contradiction, no overclaiming maturity | Manual review, ideally during release sign-off | This remains partly human because semantic contradiction is not fully machine-checkable |

## 3. Required Validation Depth

For the current stage, not every risk needs the same depth of automation.

- Repository-wide workflow checks must be automated.
- Previously bug-prone execution, lifecycle, and state paths should have focused automated coverage.
- Crash/recovery and documentation consistency may still require explicit human review even when partial automation exists.
- Live external-runtime smoke is useful, but it must not be confused with the portable or local-contract gates.

## 4. Current Minimum Release Evidence

A release candidate should be able to point to all of the following evidence classes:

- green canonical commands and CI;
- a passing tracked-candidate hygiene check showing the release can be tagged from git without forbidden local/generated artifacts;
- focused test coverage over accepted execution/state/lifecycle/extension behavior;
- explicit review of crash/recovery-sensitive paths after relevant changes;
- explicit review that docs still match shipped behavior.
- Phase 19 evidence for live proof, stress/regression coverage, operational readiness, and a completed release decision record before making release-readiness claims.

If one of these evidence classes is missing, release confidence is incomplete even if the test suite is green.

## 5. Validation Ownership Rules

- This matrix owns the list of validation classes and the expected mode per class.
- The lower-level documents still own the meaning of success or failure in their area.
- CI configuration may implement this matrix, but CI config does not become the source of truth for why a validation exists.

## 6. What This Matrix Intentionally Does Not Claim

This matrix does not claim:

- exhaustive property-based proof of all graph behavior;
- complete simulation of every App Server capability;
- elimination of all manual review before release;
- production-grade SRE coverage.

Those may become future goals, but they are not current release gates unless another owner document explicitly promotes them.

<a id="russian"></a>
Phase 19 routing note: release-readiness claims require Phase 19 real-world proof, stress/regression evidence, operational evidence, and a completed [release decision record](../20-real-world-proof-and-release/release-decision-record.md). Current local/offline Phase 18 evidence and green repository gates are necessary but not sufficient.

# Validation Matrix

Статус: нормативный.  
Владеет: текущей матрицей проверок для релизной уверенности.  
Не владеет: семантикой подсистем и точной реализацией каждого теста.  
Основные источники: [release gates](./release-gates.md), [contracts](../03-contracts/README.md), [execution](../04-execution/README.md), [state](../05-state/README.md), [lifecycle](../07-lifecycle/README.md), [extensions](../08-extensions/README.md).

## 1. Назначение

Эта матрица делает validation surface Phase 11 явной. Она не дает работе по качеству превратиться в неотслеживаемую смесь скриптов, интуиции ревьюера и случайно запомненных edge cases.

## 2. Классы проверок

| Область проверки | Что нужно защищать | Ожидаемый текущий режим проверки | Примечания |
| --- | --- | --- | --- |
| Целостность toolchain | здоровье TypeScript compile, lint compliance, test runner health, build health | Автоматически в канонических командах и CI | Операционно принадлежит [release gates](./release-gates.md) |
| Git candidate hygiene | Релизные доказательства происходят из tracked или staged product paths, а не из untracked local work; forbidden local/generated artifacts остаются вне taggable candidate | Автоматически через `pnpm release-candidate:check` перед release staging/sign-off | `dist/`, `.local/`, `subagent_tasks/`, package archives, DB/log/temp artifacts и stale `contracts/typescript/*.js` не должны становиться tracked candidate content |
| Валидность portable contracts | принимаемая структура agent file, ограничения параметров, правила output schema, форма MCP payload | Автоматически | Основные owner-docs живут в [contracts](../03-contracts/README.md) |
| Корректность execution | sequential graph execution, outcomes, final output, direct child-run behavior | Автоматически | Основные owner-docs живут в [execution](../04-execution/README.md) |
| Границы resume и interruption | explicit resume rules, interruption handling, blocked prompt persistence, revision pinning | Автоматически плюс focused manual review при изменении семантики | Основные owner-docs живут в [state](../05-state/README.md) и [interaction](../06-interaction/README.md) |
| Поведение crash и durability | atomic writes, порядок metadata, recovery invariants, durable local state | Автоматически там, где это практично, плюс focused manual review | Основные owner-docs живут в [state](../05-state/README.md) |
| Корректность lifecycle | поведение registry, drafts, live resolution, deploy semantics, trigger/event persistence | Автоматически | Основные owner-docs живут в [lifecycle](../07-lifecycle/README.md) |
| Корректность extensions | builder draft-only behavior, runtime-source gating, honesty memory bindings, уже реализованные subagent rules | Автоматически | Основные owner-docs живут в [extensions](../08-extensions/README.md) и [subagent docs](../02-architecture/subagent-orchestration-model.md) |
| Согласованность integrated product-flow | Cross-subsystem flows, которые объединяют lifecycle, builder output, runtime features, interaction, memory и managed subagents, должны сохранять честные границы каждой подсистемы | Automated local/offline integration coverage теперь существует через `tests/integration/phase18-integrated-product-flows.test.ts`; live external, stress и release-readiness validation остаются evidence для Phase 19 | Основной owner живет в [Phase 18 Integrated Product Flows](../19-integrated-product-flows/README.md); этот класс ниже Phase 19 external proof, stress proof и release-readiness evidence и не заменяет их |
| Honest behavior adapter-а | Codex App Server adapter не должен заявлять неподдерживаемое поведение; текущие capability gates должны оставаться правдивыми | Автоматически плюс focused manual review | Основные owner-docs живут в [runtime integration model](../02-architecture/runtime-integration-model.md) и [runtime adapter contract](../03-contracts/runtime-adapter-contract.md) |
| Стабильность CLI workflow | принятые команды, аргументы и release-facing workflows остаются согласованными | Автоматически плюс focused manual review для измененного UX | Правила интерфейса зависят от уже принятого поведения, а не от нового синтаксиса Phase 11 |
| Согласованность документации | owner-doc links, отсутствие material contradictions, отсутствие overclaiming maturity | Manual review, ideally во время release sign-off | Эта часть пока частично человеческая, потому что semantic contradiction не полностью machine-checkable |

## 3. Требуемая глубина проверки

На текущем этапе не каждому риску нужна одинаковая глубина автоматизации.

- Repository-wide workflow checks должны быть автоматизированы.
- Execution, lifecycle и state-пути, где уже были или вероятны баги, должны иметь focused automated coverage.
- Crash/recovery и consistency документации все еще могут требовать явного human review даже при наличии частичной автоматизации.
- Live external-runtime smoke полезен, но его нельзя путать с portable или local-contract gates.

## 4. Минимальный набор релизных доказательств

Release candidate должен уметь показать все следующие классы доказательств:

- зеленые канонические команды и CI;
- passing tracked-candidate hygiene check, показывающий, что релиз можно tag из git без forbidden local/generated artifacts;
- focused test coverage для принятого поведения execution/state/lifecycle/extensions;
- явный review crash/recovery-sensitive путей после релевантных изменений;
- явный review того, что docs по-прежнему соответствуют shipped behavior.

Если хотя бы один из этих классов доказательств отсутствует, уверенность в релизе неполная, даже если test suite зеленый.

## 5. Правила владения validation

- Эта матрица владеет списком классов проверок и ожидаемым режимом проверки по каждому классу.
- Lower-level документы по-прежнему владеют смыслом success или failure в своей области.
- CI configuration может реализовывать эту матрицу, но сам CI config не становится источником истины о том, зачем конкретная проверка существует.

## 6. Чего эта матрица намеренно не утверждает

Эта матрица не утверждает:

- exhaustive property-based proof всего graph behavior;
- полную симуляцию каждой возможности App Server;
- устранение любого manual review перед релизом;
- production-grade SRE coverage.

Все это может стать будущими целями, но не является текущими release gates, пока другой owner-doc явно не поднимет их до этого статуса.
