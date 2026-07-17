# Appendix A. Source Ledger

## Internal Dennett specifications

**[S01] Dennett Functional Concept.** Product vision, ambient microphone/screen, projects, voice, messages, portability and adaptive principles.  
`../00_Dennett_Functional_Concept.md`

**[S02] Dennett Specification Index and Shared Contracts.** Canonical ownership and boundary contracts.  
`01_Dennett_Specification_Index_and_Shared_Contracts.md`

**[S03] Dennett Memory Fabric 1.2.** Evidence, event ledger, project memory, sensory ingest, retention, deletion and retrieval.  
`../10_Dennett_Memory_Fabric.md`

**[S04] Dennett Agentic Control Fabric 1.1.** Project sessions, single-agent-first execution, Tasks/Runs, effects and completion.  
`../20_Dennett_Agentic_Control_Fabric.md`

**[S05] Dennett Trust, Identity, Autonomy and Permissions.** Identity, grants, effects, voice assurance, secrets, import trust and recovery foundations.  
`30_Dennett_Trust_Identity_Autonomy_and_Permissions.md`

**[S06] Dennett Voice and Ambient Interaction Fabric.** Voice sessions, ambient edge, turn-taking and source behavior.  
`40_Dennett_Voice_and_Ambient_Interaction_Fabric.md`

**[S07] Dennett Capabilities, Providers and Integrations.** Connectors, skills, packages, providers, local backends and capability lifecycle.  
`41_Dennett_Capabilities_Providers_and_Integrations.md`

**[S08] Dennett Server Runtime, Events, Sync and Portability.** Head, devices, events, effects, sync, backup and recovery runtime.  
`50_Dennett_Server_Runtime_Events_Sync_and_Portability.md`

**[S09] Dennett Desktop Application Business Logic.** Desktop workbench, projects, Inbox, Radar, memory, artifacts and system controls.  
`60_Dennett_Desktop_Application_Business_Logic.md`

**[S10] Dennett Mobile Application Business Logic.** Interruptible mobile remote, capture, approvals, offline and device controls.  
`61_Dennett_Mobile_Application_Business_Logic.md`

**[S11] Dennett End-to-End Validation and Architecture Handoff.** Gap ledger, E2E requirements, quality scenarios and architecture readiness.  
`70_Dennett_End_to_End_Validation_and_Architecture_Handoff.md`

## Ambient capture, privacy and consent

**[S12] Android foreground service types — microphone.** Platform constraints for background microphone and while-in-use permissions.  
https://developer.android.com/develop/background-work/services/fgs/service-types

**[S13] Android MediaProjection.** User consent and lifecycle constraints for screen capture/projection sessions.  
https://developer.android.com/media/grow/media-projection

**[S14] Windows Graphics Capture.** Windows APIs and user-visible selection/capture model.  
https://learn.microsoft.com/en-us/windows/apps/develop/media-authoring-processing/screen-capture

**[S15] Screenpipe.** Event-driven local screen/audio capture reference with OCR/accessibility and search. Used as implementation evidence, not mandatory dependency.  
https://github.com/mediar-ai/screenpipe

**[S16] NIST Privacy Framework.** Risk-based privacy management enabling product utility and innovation.  
https://www.nist.gov/privacy-framework

**[S17] EDPB Guidelines on Virtual Voice Assistants.** Privacy/data-protection considerations for voice assistants.  
https://www.edpb.europa.eu/our-work-tools/our-documents/guidelines/guidelines-022021-virtual-voice-assistants_en

**[S18] Meaningful verbal consent research.** Evidence that spoken consent UX requires clarity and context rather than treating silence as agreement.  
https://dl.acm.org/doi/10.1145/3544548.3580711

**[S51] Apple `UIBackgroundModes`.** Platform declaration for supported background execution categories; actual microphone behavior remains permission- and lifecycle-bound.  
https://developer.apple.com/documentation/bundleresources/information-property-list/uibackgroundmodes

**[S52] Apple ReplayKit.** iOS/iPadOS screen recording and broadcast capture surface.  
https://developer.apple.com/documentation/replaykit

**[S53] Apple ScreenCaptureKit.** Native macOS screen and audio capture framework.  
https://developer.apple.com/documentation/screencapturekit

## External communication and idempotency

**[S19] TDLib Getting Started.** Asynchronous Telegram client, local storage/data consistency, updates and `updateMessageSendSucceeded`.  
https://core.telegram.org/tdlib/getting-started

**[S20] Telegram Bot API.** Request/update semantics, update IDs, webhook limitations and result states.  
https://core.telegram.org/bots/api

**[S21] Gmail API Drafts.** Draft as a distinct resource before sending.  
https://developers.google.com/gmail/api/guides/drafts

**[S22] Microsoft Graph `sendMail`.** Provider send acceptance semantics and permissions.  
https://learn.microsoft.com/en-us/graph/api/user-sendmail

**[S23] AWS Builders’ Library — Making retries safe with idempotent APIs.** Caller-provided request IDs, same-intent checks and safe retries.  
https://aws.amazon.com/builders-library/making-retries-safe-with-idempotent-APIs/

## Project and artifact lifecycle

**[S24] GitHub Archiving Repositories.** Reversible read-only archive semantics distinct from deletion.  
https://docs.github.com/en/repositories/archiving-a-github-repository/archiving-repositories

**[S25] GitHub Deleting a Repository.** Consequences of remote deletion, private forks and limited restore.  
https://docs.github.com/en/repositories/creating-and-managing-repositories/deleting-a-repository

**[S26] Git Worktree.** Multiple linked working trees and branch isolation.  
https://git-scm.com/docs/git-worktree

**[S27] W3C PROV Overview.** Entities, activities, agents, derivation and provenance interoperability.  
https://www.w3.org/TR/prov-overview/

**[S28] Amazon S3 Versioning.** Retaining multiple object versions and recovery from overwrite/deletion.  
https://docs.aws.amazon.com/AmazonS3/latest/userguide/Versioning.html

**[S29] GitHub Releases.** Named software releases, tags, notes and assets.  
https://docs.github.com/en/repositories/releasing-projects-on-github/about-releases

## Updates and compatibility

**[S30] The Update Framework.** Secure software update metadata and compromise-resilient distribution principles.  
https://theupdateframework.io/

**[S31] Semantic Versioning 2.0.0.** Public API compatibility signalling and immutable released versions.  
https://semver.org/

**[S32] Protocol Buffers — Updating a Message Type.** Wire-safe, unsafe and conditionally compatible changes and unknown fields.  
https://protobuf.dev/programming-guides/proto3/#updating

**[S33] Kubernetes Version Skew Policy.** Explicit compatibility windows between distributed components.  
https://kubernetes.io/releases/version-skew-policy/

## Identity and recovery

**[S34] NIST SP 800-63B.** Authentication assurance, authenticators, recovery and lifecycle principles.  
https://pages.nist.gov/800-63-4/sp800-63b.html

**[S35] Apple Platform Security — Advanced Data Protection for iCloud.** E2EE recovery responsibility, recovery contacts and recovery keys.  
https://support.apple.com/guide/security/advanced-data-protection-for-icloud-sec973254c5f/web

**[S36] 1Password Secret Key.** User-held secret/recovery kit and limits of provider recovery.  
https://support.1password.com/secret-key-security/

**[S37] Bitwarden Emergency Access.** Trusted emergency contacts, waiting period and access policy.  
https://bitwarden.com/help/emergency-access/

## Resources, telemetry and search

**[S38] FOCUS — FinOps Open Cost & Usage Specification.** Vendor-neutral normalization across AI, cloud, SaaS and other technology usage/cost data.  
https://focus.finops.org/

**[S39] OpenTelemetry Semantic Conventions.** Common names and meanings for traces, metrics, logs, GenAI, hardware and devices.  
https://opentelemetry.io/docs/specs/semconv/

**[S40] Elasticsearch Reciprocal Rank Fusion.** Rank fusion across different retrievers without calibrated scores.  
https://www.elastic.co/docs/reference/elasticsearch/rest-apis/reciprocal-rank-fusion

**[S41] OpenSearch Hybrid Search.** Reference implementation pattern for combining keyword and semantic search.  
https://opensearch.org/docs/latest/vector-search/ai-search/hybrid-search/index/

## Locale and time

**[S42] IANA Time Zone Database.** Canonical timezone rule data updated for political/DST changes.  
https://www.iana.org/time-zones

**[S43] Unicode CLDR.** Locale data for dates, numbers, units, plurals and language/region display.  
https://cldr.unicode.org/

**[S44] BCP 47 / RFC 5646.** Language tags.  
https://www.rfc-editor.org/rfc/rfc5646

**[S45] RFC 3339.** Internet date/time timestamp representation.  
https://www.rfc-editor.org/rfc/rfc3339

## Portable packages and recipes

**[S46] RO-Crate Specification 1.3.** Portable linked metadata packaging for research/software artifacts.  
https://www.researchobject.org/ro-crate/specification/1.3/

**[S47] RFC 8493 — BagIt.** Directory payload, metadata tags and checksum manifests for reliable arbitrary-content transfer.  
https://www.rfc-editor.org/rfc/rfc8493

**[S48] JSON Schema Draft 2020-12.** Machine-readable JSON document/schema validation.  
https://json-schema.org/draft/2020-12/json-schema-core

**[S49] Home Assistant Core and Automation Blueprints.** Event/state/service separation and reusable user-customizable automation templates.  
https://developers.home-assistant.io/docs/architecture/core/  
https://www.home-assistant.io/docs/automation/using_blueprints/

**[S50] OCI Image Index Specification.** Higher-level manifest selecting platform-specific component manifests.  
https://github.com/opencontainers/image-spec/blob/main/image-index.md

---
