[English](#english) | [Russian](#russian)

<a id="english"></a>
# Hosted And Managed Deployment Scope

Status: canonical Stage 12 hosted/managed deployment deferral lock for Part 1 public-launch readiness. This document records that hosted and managed deployment are out of the current public-launch scope. It is not a deployment plan, production-readiness checklist, or approval to operate a hosted service.

Related documents:

- [Public Launch Readiness](./README.md)
- [Public Launch Scope](./public-launch-scope.md)
- [Security, Privacy, And Legal Foundation](./security-privacy-legal-foundation.md)
- [Operational Runbook](../20-real-world-proof-and-release/operational-runbook.md)
- [Security Policy](../../SECURITY.md)

## Stage 12 Decision

Hosted and managed deployment is explicitly deferred from the current public-launch scope.

The current launch target remains CLI/package-first. The current evidence supports local repository checkout, controlled local package artifact proof, local state, user-owned runtime/provider accounts, and local operator procedures. It does not support operating Dennett as a SaaS, hosted application, managed service, cloud deployment, or production multi-tenant platform.

This deferral is scope control, not a statement that hosted deployment is impossible. A later scope decision may reopen hosted or managed deployment only after it names the artifact, environment, tenant model, operational owner, security/privacy/legal posture, rollout and rollback path, observability/audit model, support model, and live evidence requirements.

## Code And Configuration Evidence

Stage 12 records the following current-state evidence from the hosted/deployment audits:

- No hosted Dennett server is implemented for the public-launch product surface.
- No cloud deployment configuration is present for Dennett, including Docker, Kubernetes, Terraform, Vercel, Fly, Render, Railway, or equivalent production deployment definitions.
- No deploy workflow exists for cloud, SaaS, managed, or production-hosted rollout.
- No multi-tenant isolation model, hosted account/auth model, server-side secret store, hosted telemetry model, hosted audit-log model, hosted support-access model, or incident-response model is implemented.
- Local `deploy` terminology means draft-to-live lifecycle promotion inside the agent lifecycle, not cloud deployment or hosted rollout.
- Package proof remains controlled local `.tgz` proof; it is not a hosted artifact, installer, container, public registry publication, or managed distribution channel.
- SQLite and provider registrations remain local/user-owned operational state, not hosted storage, tenant data stores, or a distributed operational backend.

External or cached third-party sources under local working directories do not create a Dennett hosted deployment path. Only repository-owned product code, docs, package metadata, scripts, CI, and release artifacts can establish public-launch scope.

## Hosted Gap Matrix

| Hosted/managed area | Current status | Required before scope can reopen |
| --- | --- | --- |
| Hosted artifact | Not started. | Name the server, service, container, installer, package, or managed artifact and prove build/release integrity. |
| Deployment environment | Not started. | Select and document the cloud/runtime environment, regions, networking, rollout path, and environment ownership. |
| Accounts/auth | Not started. | Define user identity, tenant accounts, admin roles, auth lifecycle, account recovery, and abuse handling. |
| Tenant isolation | Not started. | Prove isolation for state, runs, memory, runtime sessions, logs, artifacts, Builder drafts, and support access. |
| Server-side secrets | Not started. | Provide secret storage, rotation, redaction, access control, audit, and break-glass rules. |
| Support access | Not started. | Define what support can access, approval rules, redaction, logging, and customer-visible boundaries. |
| Telemetry/observability/audit logs | Not started. | Define user notice, opt-in or control policy, event taxonomy, retention, access, redaction, alerting, and audit integrity. |
| Deletion/export/legal hold | Not started. | Define deletion, export, backup, restore, retention, legal hold, and provider data-processing behavior. |
| Incident response | Not started. | Define detection, severity, escalation, containment, notification, evidence preservation, and post-incident review. |
| Rollback/disablement | Not started. | Prove hosted rollback, rollout halt, tenant disablement, feature disablement, and post-rollback verification. |
| Operational owner | Not assigned. | Name accountable operator roles, escalation paths, maintenance windows, support coverage, and decision authority. |

## Explicitly Forbidden Claims

Until a later hosted/managed scope decision and evidence set replace this deferral, do not claim:

- Dennett is a SaaS, hosted service, managed service, production web application, or production platform.
- Hosted or managed deployment is in current public-launch scope.
- A cloud deployment, deploy workflow, hosted artifact, container image, hosted server, or managed runtime exists for public launch.
- Uptime, availability, service-level agreement, operational support coverage, incident response, or hosted status-page readiness.
- Multi-tenant readiness, hosted tenant isolation, hosted account/auth readiness, support-access safety, or server-side secret-management readiness.
- Hosted telemetry, hosted observability, hosted audit logging, hosted analytics, or compliance/audit readiness.
- Hosted deletion, export, backup, restore, legal hold, disaster recovery, hosted rollback, hosted disablement, or production-load readiness.
- Local lifecycle `deploy` is equivalent to cloud deployment.
- Stage 11 local package proof proves hosted rollout, public registry publication, managed distribution, or production deployment.

## Required Scope Reopen Inputs

A later hosted/managed task must start by replacing this document or adding an explicitly linked successor that includes:

- exact product surface and user-visible behavior;
- artifact and deployment environment;
- ownership and support model;
- tenant, auth, secret, and data-processing model;
- telemetry, observability, audit, and incident model;
- deletion, export, retention, backup, restore, and legal-hold behavior;
- rollout, rollback, disablement, recovery, and post-rollback verification;
- live proof, load/stress criteria, failure evidence, and release gates;
- updated public claims and forbidden claims.

<a id="russian"></a>
# Область hosted и managed deployment

Статус: канонический Stage 12 deferral lock для hosted/managed deployment в Part 1 public-launch readiness. Этот документ фиксирует, что hosted и managed deployment не входят в текущую область публичного запуска. Это не план деплоя, не production-readiness checklist и не разрешение эксплуатировать hosted service.

Связанные документы:

- [Public Launch Readiness](./README.md)
- [Public Launch Scope](./public-launch-scope.md)
- [Security, Privacy, And Legal Foundation](./security-privacy-legal-foundation.md)
- [Operational Runbook](../20-real-world-proof-and-release/operational-runbook.md)
- [Security Policy](../../SECURITY.md)

## Решение Stage 12

Hosted и managed deployment явно отложены и не входят в текущий public-launch scope.

Текущая цель остается CLI/package-first. Доказательства покрывают локальный checkout репозитория, контролируемое локальное доказательство package artifact, локальное состояние, пользовательские runtime/provider accounts и локальные operator procedures. Они не доказывают SaaS, hosted application, managed service, cloud deployment или production multi-tenant platform.

Позднее scope decision может вернуть hosted или managed deployment в область работ только после фиксации artifact, environment, tenant model, operational owner, security/privacy/legal posture, rollout and rollback path, observability/audit model, support model и требований к live evidence.

## Доказательства по коду и конфигурации

Stage 12 фиксирует текущее состояние по результатам hosted/deployment audits:

- Hosted Dennett server для public-launch product surface не реализован.
- Cloud deployment config для Dennett отсутствует: нет Docker, Kubernetes, Terraform, Vercel, Fly, Render, Railway или эквивалентных production deployment definitions.
- Deploy workflow для cloud, SaaS, managed или production-hosted rollout отсутствует.
- Multi-tenant isolation model, hosted account/auth model, server-side secret store, hosted telemetry model, hosted audit-log model, hosted support-access model и incident-response model не реализованы.
- Локальный термин `deploy` означает draft-to-live lifecycle promotion внутри agent lifecycle, а не cloud deployment или hosted rollout.
- Package proof остается контролируемым локальным `.tgz` proof; это не hosted artifact, installer, container, public registry publication или managed distribution channel.
- SQLite и provider registrations остаются локальным/user-owned operational state, а не hosted storage, tenant data stores или distributed operational backend.

Сторонние или кешированные external sources в локальных рабочих каталогах не создают hosted deployment path для Dennett. Только repository-owned product code, docs, package metadata, scripts, CI и release artifacts могут устанавливать public-launch scope.

## Hosted gap matrix

| Hosted/managed area | Текущий статус | Что требуется до возврата в scope |
| --- | --- | --- |
| Hosted artifact | Not started. | Назвать server, service, container, installer, package или managed artifact и доказать build/release integrity. |
| Deployment environment | Not started. | Выбрать и описать cloud/runtime environment, regions, networking, rollout path и ownership. |
| Accounts/auth | Not started. | Определить user identity, tenant accounts, admin roles, auth lifecycle, account recovery и abuse handling. |
| Tenant isolation | Not started. | Доказать isolation для state, runs, memory, runtime sessions, logs, artifacts, Builder drafts и support access. |
| Server-side secrets | Not started. | Ввести secret storage, rotation, redaction, access control, audit и break-glass rules. |
| Support access | Not started. | Определить support access, approval rules, redaction, logging и customer-visible boundaries. |
| Telemetry/observability/audit logs | Not started. | Определить user notice, opt-in/control policy, event taxonomy, retention, access, redaction, alerting и audit integrity. |
| Deletion/export/legal hold | Not started. | Определить deletion, export, backup, restore, retention, legal hold и provider data-processing behavior. |
| Incident response | Not started. | Определить detection, severity, escalation, containment, notification, evidence preservation и post-incident review. |
| Rollback/disablement | Not started. | Доказать hosted rollback, rollout halt, tenant disablement, feature disablement и post-rollback verification. |
| Operational owner | Not assigned. | Назвать accountable operator roles, escalation paths, maintenance windows, support coverage и decision authority. |

## Запрещенные claims

Пока позднее hosted/managed scope decision и evidence set не заменят этот deferral, нельзя заявлять:

- Dennett является SaaS, hosted service, managed service, production web application или production platform.
- Hosted или managed deployment входит в текущий public-launch scope.
- Cloud deployment, deploy workflow, hosted artifact, container image, hosted server или managed runtime существуют для public launch.
- Uptime, availability, SLA, operational support coverage, incident response или hosted status-page readiness.
- Multi-tenant readiness, hosted tenant isolation, hosted account/auth readiness, support-access safety или server-side secret-management readiness.
- Hosted telemetry, hosted observability, hosted audit logging, hosted analytics или compliance/audit readiness.
- Hosted deletion, export, backup, restore, legal hold, disaster recovery, hosted rollback, hosted disablement или production-load readiness.
- Локальный lifecycle `deploy` эквивалентен cloud deployment.
- Stage 11 local package proof доказывает hosted rollout, public registry publication, managed distribution или production deployment.

## Что нужно для возврата scope

Поздняя hosted/managed task должна начать с замены этого документа или явно связанного successor document, где зафиксированы:

- точная product surface и user-visible behavior;
- artifact и deployment environment;
- ownership и support model;
- tenant, auth, secret и data-processing model;
- telemetry, observability, audit и incident model;
- deletion, export, retention, backup, restore и legal-hold behavior;
- rollout, rollback, disablement, recovery и post-rollback verification;
- live proof, load/stress criteria, failure evidence и release gates;
- обновленные public claims и forbidden claims.
