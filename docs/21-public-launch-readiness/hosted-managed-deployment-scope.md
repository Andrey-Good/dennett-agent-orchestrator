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
# Russian Translation Status

The previous localized duplicate section was removed because it contained mojibake. The English section above is the canonical public launch record until a reviewed Russian translation is restored.
