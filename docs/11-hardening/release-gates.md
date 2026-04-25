[English](#english) | [Русский](#russian)

<a id="english"></a>
# Release Gates

Status: normative.
Owns: the required gates for calling the current repository state release-ready.
Does not own: how each subsystem behaves internally; that remains owned by earlier sections.
Primary sources: [hardening scope](./hardening-scope.md), [technology stack](../01-foundations/technology-stack.md), [state](../05-state/README.md), [lifecycle](../07-lifecycle/README.md), [Phase 19 real-world proof and release](../20-real-world-proof-and-release/README.md).

## 1. Meaning Of "Release" In This Repository

For the current project stage, a release means a repository state that is fit to be tagged or otherwise treated as the current stable working version for contributors and local users.

It does not automatically mean:

- hosted-service production readiness;
- universal compatibility across all future runtimes;
- absence of all residual risk.

## 2. Mandatory Automated Gates

Before a repository release may be declared, all of the following commands must pass from a clean checkout on the locked workflow:

- `pnpm typecheck`
- `pnpm lint`
- `pnpm test`
- `pnpm build`
- `node --no-warnings -e "await import('node:sqlite')"`
- `node dist/src/interfaces/cli.js --help`
- `pnpm dist:check`
- `pnpm packlist:check`
- `pnpm release-candidate:check`
- `pnpm package:check`

If the canonical repository workflow later adds another required command, this document must be updated explicitly. Hidden release gates are forbidden.

## 3. Mandatory CI Gate

The repository must have CI that runs the mandatory automated gates on the canonical stack path. For the current stage, CI is expected to validate at least:

- dependency installation through the `pnpm` workflow;
- TypeScript type safety;
- Biome formatting and lint compliance;
- Vitest test execution;
- build success for the TypeScript distribution path.
- Node SQLite import availability on the locked Node version;
- generated `dist` CLI help smoke after `pnpm build`;
- distribution artifact checks through `pnpm dist:check`;
- package inventory proof through `pnpm packlist:check`, which wraps `npm pack --dry-run --json --ignore-scripts`;
- tracked/taggable release-candidate proof through `pnpm release-candidate:check`, which rejects required product paths that are only untracked locally and rejects forbidden tracked, staged, or visible untracked artifacts;
- package metadata and build-local distribution proof through `pnpm package:check`.

The generated `dist` tree is a build-local artifact. Local users and CI must create it with `pnpm build` from the checkout before running `node dist/src/interfaces/cli.js --help` or any distribution/package check; this repository must not claim that generated `dist` is tracked or already present in a clean checkout.

The release candidate must be taggable from git state, not reconstructed from a contributor's untracked working tree. Product source, contracts, tests, scripts, CI, and docs must be tracked or staged before release validation. Local scratch state such as `.local/`, `subagent_tasks/`, package archives, database/log/temp artifacts, stale `contracts/typescript/*.js`, and generated `dist/` output must remain outside the tracked candidate.

A release must not rely solely on one contributor's local machine output.

## 4. Mandatory Contract And Regression Gate

A release is blocked if there is evidence that the current code violates any accepted contract or owner-doc invariant, even when the basic command set passes.

This includes regressions in:

- portable agent-file validation;
- output validation and outcome classification;
- resume and recovery boundaries;
- registry and deploy semantics;
- builder draft-only behavior;
- runtime-source gating and capability honesty;
- subagent orchestration rules already accepted by the docs.

## 5. Crash And Recovery Gate

Release-readiness requires explicit confidence that the current implementation still respects:

- the atomic-write rules in [atomic-write-policy](../05-state/atomic-write-policy.md);
- the local-state invariants in [local-storage-model](../05-state/local-storage-model.md);
- the resume and interruption boundaries in [chat-and-resume](../05-state/chat-and-resume.md).

This confidence may come from automated tests, focused manual verification, or both, but it must be recorded in the Phase 11 validation surface rather than assumed.

## 6. Backward-Compatibility Gate

The current repository stage must preserve backward compatibility for the accepted current contract surface unless an owner-doc or ADR explicitly says otherwise.

At minimum, a release must not silently break:

- current portable agent-file structure accepted by the repository;
- current local lifecycle semantics for known agents, drafts, live revisions, and deploy;
- current CLI command behavior that is already treated as accepted user-facing workflow.

If a breaking change is intentional, the change must be documented as such before the release is considered valid.

## 7. Documentation Gate

Release-readiness requires the normative docs to stay consistent with the shipped repository behavior.

At minimum:

- new release-facing checks must be reflected in this section;
- no owner-doc may claim support for behavior the code explicitly rejects;
- no release claim may overstate the current maturity envelope from [operational-readiness](./operational-readiness.md).

## 8. Manual Sign-Off Requirements

Even with CI, the current stage still requires explicit human review of:

- whether the docs and code still agree at the owner-doc level;
- whether any known residual risk is being hidden by green automated checks;
- whether release notes or contributor guidance need updating because behavior changed inside the accepted scope.

Manual sign-off is not a substitute for automated gates. It is an additional release gate over them.

For any Phase 19 release-readiness claim, manual sign-off must be recorded in the Phase 19 [release decision record](../20-real-world-proof-and-release/release-decision-record.md). A green command summary, CI run, or local/offline Phase 18 integrated-flow test is not enough to replace that decision record.

## 9. Release Must Be Blocked When

The repository is not release-ready if any of the following are true:

- any mandatory automated gate fails;
- CI is absent or failing on the canonical path;
- known contract regressions remain unresolved;
- known crash/recovery invariants are unverified after relevant changes;
- the docs materially contradict current shipped behavior;
- a release-readiness claim lacks a completed Phase 19 release decision record;
- the release claim depends on ignoring a known blocker as if it were cosmetic.

<a id="russian"></a>
Маршрутная заметка Phase 19: любое заявление о release readiness должно иметь доказательства Phase 19 и завершенную [release decision record](../20-real-world-proof-and-release/release-decision-record.md); зеленые команды или local/offline тесты Phase 18 не могут заменить ее.

# Release Gates

Статус: нормативный.
Владеет: обязательными критериями для того, чтобы считать текущее состояние репозитория готовым к релизу.
Не владеет: тем, как каждая подсистема работает внутри; это по-прежнему принадлежит более ранним разделам.
Основные источники: [hardening scope](./hardening-scope.md), [technology stack](../01-foundations/technology-stack.md), [state](../05-state/README.md), [lifecycle](../07-lifecycle/README.md), [Phase 19 real-world proof and release](../20-real-world-proof-and-release/README.md).

## 1. Что означает "релиз" в этом репозитории

Для текущего этапа проекта релиз означает состояние репозитория, которое можно тегировать или иным образом считать текущей стабильной рабочей версией для контрибьюторов и локальных пользователей.

Это автоматически не означает:

- production readiness hosted-сервиса;
- универсальную совместимость со всеми будущими runtime;
- отсутствие любых residual risks.

## 2. Обязательные автоматические gates

Перед тем как объявить релиз репозитория, из clean checkout по locked workflow обязаны успешно проходить все следующие команды:

- `pnpm typecheck`
- `pnpm lint`
- `pnpm test`
- `pnpm build`
- `node --no-warnings -e "await import('node:sqlite')"`
- `node dist/src/interfaces/cli.js --help`
- `pnpm dist:check`
- `pnpm packlist:check`
- `pnpm release-candidate:check`
- `pnpm package:check`

Если позже в канонический workflow репозитория будет добавлена еще одна обязательная команда, этот документ нужно обновить явно. Скрытые release gates запрещены.

## 3. Обязательный CI gate

В репозитории обязан быть CI, который запускает обязательные автоматические gates на каноническом пути стека. Для текущего этапа от CI ожидается как минимум проверка:

- установки зависимостей через workflow `pnpm`;
- TypeScript type safety;
- соблюдения форматирования и lint-правил Biome;
- выполнения тестов Vitest;
- успешной сборки TypeScript distribution path.
- Node SQLite import availability на locked Node version;
- generated `dist` CLI help smoke после `pnpm build`;
- distribution artifact checks через `pnpm dist:check`;
- package inventory proof через `pnpm packlist:check`, который wraps `npm pack --dry-run --json --ignore-scripts`;
- tracked/taggable release-candidate proof через `pnpm release-candidate:check`, который отклоняет обязательные product paths, существующие только как untracked local work, и отклоняет forbidden tracked, staged или visible untracked artifacts;
- package metadata и build-local distribution proof через `pnpm package:check`.

Generated `dist` tree является build-local artifact. Локальные пользователи и CI должны создавать его через `pnpm build` из checkout перед запуском `node dist/src/interfaces/cli.js --help` или любых distribution/package checks; репозиторий не должен утверждать, что generated `dist` tracked или уже присутствует в clean checkout.

Release candidate должен быть taggable из git state, а не реконструироваться из untracked working tree одного контрибьютора. Product source, contracts, tests, scripts, CI и docs должны быть tracked или staged перед release validation. Local scratch state вроде `.local/`, `subagent_tasks/`, package archives, database/log/temp artifacts, stale `contracts/typescript/*.js` и generated `dist/` output должны оставаться вне tracked candidate.

Релиз не должен опираться только на результат, полученный на локальной машине одного контрибьютора.

## 4. Обязательный contract и regression gate

Релиз блокируется, если есть признаки того, что текущий код нарушает любой принятый контракт или invariant из owner-docs, даже если базовый набор команд проходит.

Это включает регрессии в:

- portable agent-file validation;
- output validation и outcome classification;
- resume и recovery boundaries;
- registry и deploy semantics;
- builder draft-only behavior;
- runtime-source gating и honesty по capabilities;
- правила subagent orchestration, уже принятые документацией.

## 5. Gate по crash и recovery

Release-readiness требует явной уверенности в том, что текущая реализация все еще соблюдает:

- правила атомарной записи из [atomic-write-policy](../05-state/atomic-write-policy.md);
- инварианты local state из [local-storage-model](../05-state/local-storage-model.md);
- границы resume и interruption из [chat-and-resume](../05-state/chat-and-resume.md).

Эта уверенность может происходить из automated tests, focused manual verification или их комбинации, но она должна быть зафиксирована в Phase 11 validation surface, а не подразумеваться молча.

## 6. Gate по backward compatibility

Текущий этап репозитория обязан сохранять backward compatibility для принятой текущей поверхности контрактов, если owner-doc или ADR явно не говорит обратное.

Как минимум, релиз не должен молча ломать:

- текущую portable agent-file structure, принимаемую репозиторием;
- текущую local lifecycle semantics для known agents, drafts, live revisions и deploy;
- текущее поведение CLI-команд, которое уже считается принятым user-facing workflow.

Если breaking change намеренный, он должен быть задокументирован до того, как релиз будет считаться валидным.

## 7. Documentation gate

Release-readiness требует, чтобы нормативная документация оставалась согласованной с поставляемым поведением репозитория.

Как минимум:

- новые release-facing checks должны быть отражены в этом разделе;
- ни один owner-doc не должен заявлять поддержку поведения, которое код явно отклоняет;
- ни одно релизное заявление не должно преувеличивать текущую рамку зрелости из [operational-readiness](./operational-readiness.md).

## 8. Требования к ручному sign-off

Даже при наличии CI текущий этап все еще требует явного human review по следующим вопросам:

- по-прежнему ли docs и code согласованы на уровне owner-docs;
- не скрывает ли зеленый automated check известный residual risk;
- нужно ли обновить release notes или contributor guidance, потому что поведение изменилось внутри уже принятого scope.

Manual sign-off не заменяет automated gates. Это дополнительный релизный gate поверх них.

Для любого заявления о release readiness в Phase 19 ручной sign-off должен быть зафиксирован в Phase 19 [release decision record](../20-real-world-proof-and-release/release-decision-record.md). Зеленая сводка команд, CI run или local/offline Phase 18 integrated-flow test не заменяют эту decision record.

## 9. Релиз обязан быть заблокирован, когда

Репозиторий не готов к релизу, если верно хотя бы одно из следующего:

- любой обязательный automated gate падает;
- CI отсутствует или падает на каноническом пути;
- известные contract regressions остаются неисправленными;
- известные crash/recovery invariants не верифицированы после релевантных изменений;
- docs материально противоречат текущему shipped behavior;
- заявление о release readiness не имеет завершенной Phase 19 release decision record;
- релизное заявление опирается на игнорирование известного blocker как будто он косметический.
