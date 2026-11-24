//! Thin CLI facade (`dennett-memory-cli`) backed by [`MemoryClient`] -> `dennett-memoryd`.
//!
//! Architectural invariants (Architecture 82 §7, V06):
//! - Uses the exact same domain implementation as API and MCP by calling `dennett-memoryd` over HTTP.
//! - Runs directly against standalone `dennett-memoryd` without requiring `dennett-head`.
//! - Reads bearer credentials STRICTLY from `DENNETT_MEMORY_TOKEN` (never accepts a `--token` flag).
//! - Treats `--agent` solely as an untrusted display label (`author_label`), never as principal proof.
//! - Supports `--help`, `--json`, bounded `--stdin` reading, stable exit codes, and secret redaction.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::PathBuf;
use std::process::ExitCode;
use std::str::FromStr;

use dennett_contracts::redact::{redact_secrets, sanitize_cli_arg};
use dennett_contracts::{
    AdmissionStatus, ClaimId, ConflictId, ConflictStatus, ConsolidationJobId, ConsolidationJobType,
    CorrectNoteRequest, CorrelationId, CreateClaimRequest, CreateConsolidationJobRequest,
    CreateDocumentRequest, CreateNoteRequest, DennettError, DocumentId, EpistemicStatus,
    EvidenceId, EvidenceLinkInput, EvidenceRelationship, ExportRequest, ForgetRequest,
    ForgetTargetType, IdempotencyKey, ImportRequest, IngestEvidenceRequest,
    MAX_EVIDENCE_CONTENT_BYTES, MAX_PACKAGE_JSON_BYTES, MemoryEventId, MemoryNoteId, MemoryScope,
    ResolveConflictRequest, SearchNotesRequest, SupersedeClaimRequest, TimestampMs,
    TriggerIndexRebuildRequest, UpdateDocumentRequest,
};
use dennett_memory_client::MemoryClient;
use serde::Serialize;
use uuid::Uuid;

const DEFAULT_MEMORY_URL: &str = "http://127.0.0.1:18080";
const ENV_MEMORY_URL: &str = "DENNETT_MEMORY_URL";
const ENV_MEMORY_TOKEN: &str = "DENNETT_MEMORY_TOKEN";

#[derive(Debug, Clone, PartialEq, Eq)]
struct GlobalOptions {
    url: String,
    json: bool,
    correlation_id: Option<CorrelationId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TextSource {
    Inline(String),
    Stdin,
    File(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WriteArgs {
    global: GlobalOptions,
    scope: MemoryScope,
    title: String,
    body: TextSource,
    tags: Vec<String>,
    evidence_links: Vec<EvidenceLinkInput>,
    agent_label: Option<String>,
    admission_status: Option<AdmissionStatus>,
    salience: Option<u8>,
    topic_key: Option<String>,
    idempotency_key: IdempotencyKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CorrectArgs {
    global: GlobalOptions,
    note_id: MemoryNoteId,
    scope: MemoryScope,
    expected_revision: i64,
    title: String,
    body: TextSource,
    rationale: String,
    tags: Vec<String>,
    evidence_links: Vec<EvidenceLinkInput>,
    agent_label: Option<String>,
    idempotency_key: IdempotencyKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClaimCreateArgs {
    global: GlobalOptions,
    scope: MemoryScope,
    topic_key: String,
    claim_key: Option<String>,
    natural_language: String,
    epistemic_status: EpistemicStatus,
    caveats: Vec<String>,
    valid_from: Option<TimestampMs>,
    valid_to: Option<TimestampMs>,
    source_note_id: Option<MemoryNoteId>,
    idempotency_key: IdempotencyKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClaimSupersedeArgs {
    global: GlobalOptions,
    claim_id: ClaimId,
    scope: Option<MemoryScope>,
    topic_key: Option<String>,
    claim_key: Option<String>,
    natural_language: String,
    epistemic_status: EpistemicStatus,
    caveats: Vec<String>,
    valid_from: Option<TimestampMs>,
    valid_to: Option<TimestampMs>,
    resolution_event_id: Option<MemoryEventId>,
    idempotency_key: IdempotencyKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConflictResolveArgs {
    global: GlobalOptions,
    conflict_id: ConflictId,
    scope: Option<MemoryScope>,
    winning_claim_id: Option<ClaimId>,
    target_status: Option<ConflictStatus>,
    resolution_note: String,
    idempotency_key: IdempotencyKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DocumentCreateArgs {
    global: GlobalOptions,
    scope: MemoryScope,
    document_key: String,
    title: String,
    body: TextSource,
    provenance_claim_ids: Vec<ClaimId>,
    provenance_note_ids: Vec<MemoryNoteId>,
    idempotency_key: IdempotencyKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DocumentUpdateArgs {
    global: GlobalOptions,
    document_id: DocumentId,
    scope: Option<MemoryScope>,
    expected_version: i32,
    title: String,
    body: TextSource,
    provenance_claim_ids: Vec<ClaimId>,
    provenance_note_ids: Vec<MemoryNoteId>,
    diff_summary: Option<String>,
    idempotency_key: IdempotencyKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ForgetArgs {
    global: GlobalOptions,
    scope: MemoryScope,
    target_type: ForgetTargetType,
    target_id: Uuid,
    reason: String,
    idempotency_key: IdempotencyKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CliAction {
    Help,
    Version,
    Health {
        global: GlobalOptions,
    },
    IngestEvidence {
        global: GlobalOptions,
        scope: MemoryScope,
        media_type: String,
        content: TextSource,
        source_uri: Option<String>,
        idempotency_key: IdempotencyKey,
    },
    Write(Box<WriteArgs>),
    Get {
        global: GlobalOptions,
        note_id: MemoryNoteId,
    },
    History {
        global: GlobalOptions,
        note_id: MemoryNoteId,
    },
    Correct(Box<CorrectArgs>),
    Search {
        global: GlobalOptions,
        scope: Option<MemoryScope>,
        query: Option<String>,
        limit: Option<u32>,
        include_history: bool,
    },
    Event {
        global: GlobalOptions,
        event_id: MemoryEventId,
    },
    ClaimCreate(Box<ClaimCreateArgs>),
    ClaimGet {
        global: GlobalOptions,
        claim_id: ClaimId,
    },
    ClaimSupersede(Box<ClaimSupersedeArgs>),
    ClaimList {
        global: GlobalOptions,
        scope: Option<MemoryScope>,
        topic_key: Option<String>,
    },
    ConflictList {
        global: GlobalOptions,
        scope: Option<MemoryScope>,
    },
    ConflictResolve(Box<ConflictResolveArgs>),
    DocumentList {
        global: GlobalOptions,
        scope: Option<MemoryScope>,
    },
    DocumentCreate(Box<DocumentCreateArgs>),
    DocumentGet {
        global: GlobalOptions,
        document_id: DocumentId,
    },
    DocumentUpdate(Box<DocumentUpdateArgs>),
    DocumentVersions {
        global: GlobalOptions,
        document_id: DocumentId,
    },
    Forget(Box<ForgetArgs>),
    ForgetTombstones {
        global: GlobalOptions,
        scope: Option<MemoryScope>,
    },
    Export {
        global: GlobalOptions,
        scope: MemoryScope,
        format_version: Option<String>,
        output: Option<PathBuf>,
    },
    Import {
        global: GlobalOptions,
        scope: MemoryScope,
        input: TextSource,
    },
    IndexStatus {
        global: GlobalOptions,
        scope: Option<MemoryScope>,
    },
    IndexRebuild {
        global: GlobalOptions,
        scope: MemoryScope,
    },
    Stats {
        global: GlobalOptions,
        scope: Option<MemoryScope>,
    },
    ConsolidationList {
        global: GlobalOptions,
        scope: Option<MemoryScope>,
    },
    ConsolidationCreate {
        global: GlobalOptions,
        scope: MemoryScope,
        job_type: ConsolidationJobType,
        idempotency_key: Option<IdempotencyKey>,
    },
    ConsolidationPause {
        global: GlobalOptions,
        job_id: ConsolidationJobId,
    },
    ConsolidationResume {
        global: GlobalOptions,
        job_id: ConsolidationJobId,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
    let default_url = env::var(ENV_MEMORY_URL)
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| String::from(DEFAULT_MEMORY_URL));

    let token = env::var(ENV_MEMORY_TOKEN)
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());

    let action = match parse_cli(env::args().skip(1), &default_url) {
        Ok(a) => a,
        Err(err) => {
            emit_error(&err, false, token.as_deref());
            return exit_code_for_error(&err);
        }
    };

    let (json_mode, res) = match action {
        CliAction::Help => {
            print_usage();
            return ExitCode::SUCCESS;
        }
        CliAction::Version => {
            println!("dennett-memory-cli {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        other => {
            let json = is_json_mode(&other);
            (json, execute_action(other, token.clone()).await)
        }
    };

    match res {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            emit_error(&err, json_mode, token.as_deref());
            exit_code_for_error(&err)
        }
    }
}

fn is_json_mode(action: &CliAction) -> bool {
    match action {
        CliAction::Health { global }
        | CliAction::IngestEvidence { global, .. }
        | CliAction::Get { global, .. }
        | CliAction::History { global, .. }
        | CliAction::Search { global, .. }
        | CliAction::Event { global, .. }
        | CliAction::ClaimGet { global, .. }
        | CliAction::ClaimList { global, .. }
        | CliAction::ConflictList { global, .. }
        | CliAction::DocumentList { global, .. }
        | CliAction::DocumentGet { global, .. }
        | CliAction::DocumentVersions { global, .. }
        | CliAction::ForgetTombstones { global, .. }
        | CliAction::Export { global, .. }
        | CliAction::Import { global, .. }
        | CliAction::IndexStatus { global, .. }
        | CliAction::IndexRebuild { global, .. }
        | CliAction::Stats { global, .. }
        | CliAction::ConsolidationList { global, .. }
        | CliAction::ConsolidationCreate { global, .. }
        | CliAction::ConsolidationPause { global, .. }
        | CliAction::ConsolidationResume { global, .. } => global.json,
        CliAction::Write(a) => a.global.json,
        CliAction::Correct(a) => a.global.json,
        CliAction::ClaimCreate(a) => a.global.json,
        CliAction::ClaimSupersede(a) => a.global.json,
        CliAction::ConflictResolve(a) => a.global.json,
        CliAction::DocumentCreate(a) => a.global.json,
        CliAction::DocumentUpdate(a) => a.global.json,
        CliAction::Forget(a) => a.global.json,
        CliAction::Help | CliAction::Version => false,
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "central command dispatcher maps every CLI subcommand directly to MemoryClient"
)]
async fn execute_action(action: CliAction, token: Option<String>) -> Result<(), DennettError> {
    match action {
        CliAction::Health { global } => exec_health(&global).await,
        CliAction::IngestEvidence {
            global,
            scope,
            media_type,
            content,
            source_uri,
            idempotency_key,
        } => {
            exec_ingest_evidence(
                &global,
                token,
                scope,
                media_type,
                content,
                source_uri,
                idempotency_key,
            )
            .await
        }
        CliAction::Write(args) => {
            let WriteArgs {
                global,
                scope,
                title,
                body,
                tags,
                evidence_links,
                agent_label,
                admission_status,
                salience,
                topic_key,
                idempotency_key,
            } = *args;
            let req = CreateNoteRequest {
                scope,
                title,
                body: resolve_text(body, MAX_EVIDENCE_CONTENT_BYTES)?,
                tags,
                observed_at: None,
                evidence_links,
                author_label: agent_label,
                admission_status,
                salience,
                topic_key,
                idempotency_key,
            };
            exec_write(&global, token, req).await
        }
        CliAction::Get { global, note_id } => exec_get(&global, token, note_id).await,
        CliAction::History { global, note_id } => exec_history(&global, token, note_id).await,
        CliAction::Correct(args) => {
            let CorrectArgs {
                global,
                note_id,
                scope,
                expected_revision,
                title,
                body,
                rationale,
                tags,
                evidence_links,
                agent_label,
                idempotency_key,
            } = *args;
            let req = CorrectNoteRequest {
                note_id,
                scope,
                expected_revision,
                title,
                body: resolve_text(body, MAX_EVIDENCE_CONTENT_BYTES)?,
                tags,
                rationale,
                evidence_links,
                author_label: agent_label,
                idempotency_key,
            };
            exec_correct(&global, token, req).await
        }
        CliAction::Search {
            global,
            scope,
            query,
            limit,
            include_history,
        } => {
            exec_search(
                &global,
                token,
                SearchNotesRequest {
                    scope,
                    query,
                    limit,
                    include_history,
                },
            )
            .await
        }
        CliAction::Event { global, event_id } => exec_event(&global, token, event_id).await,
        CliAction::ClaimCreate(args) => exec_claim_create(*args, token).await,
        CliAction::ClaimGet { global, claim_id } => {
            let client = build_auth_client(&global.url, token)?;
            let claim = client
                .get_claim(claim_id, global.correlation_id.as_ref())
                .await?;
            if global.json {
                print_json(&claim)?;
            } else {
                println!(
                    "claim_id={} scope={} topic={} status={} text={}",
                    claim.claim_id,
                    claim.scope,
                    claim.topic_key,
                    claim.epistemic_status,
                    claim.natural_language
                );
            }
            Ok(())
        }
        CliAction::ClaimSupersede(args) => exec_claim_supersede(*args, token).await,
        CliAction::ClaimList {
            global,
            scope,
            topic_key,
        } => {
            let client = build_auth_client(&global.url, token)?;
            let resp = match (scope.as_ref(), topic_key.as_deref()) {
                (Some(sc), Some(topic)) => {
                    client
                        .list_claims_by_topic(sc, topic, global.correlation_id.as_ref())
                        .await?
                }
                _ => {
                    client
                        .list_claims(
                            scope.as_ref(),
                            topic_key.as_deref(),
                            global.correlation_id.as_ref(),
                        )
                        .await?
                }
            };
            if global.json {
                print_json(&resp)?;
            } else {
                println!("claims={}", resp.claims.len());
                for c in &resp.claims {
                    println!(
                        "  claim_id={} scope={} topic={} status={} text={}",
                        c.claim_id, c.scope, c.topic_key, c.epistemic_status, c.natural_language
                    );
                }
            }
            Ok(())
        }
        CliAction::ConflictList { global, scope } => {
            let client = build_auth_client(&global.url, token)?;
            let resp = client
                .list_open_conflicts(scope.as_ref(), global.correlation_id.as_ref())
                .await?;
            if global.json {
                print_json(&resp)?;
            } else {
                println!("open_conflicts={}", resp.conflicts.len());
                for c in &resp.conflicts {
                    println!(
                        "  conflict_id={} scope={} type={} status={} candidates={}",
                        c.conflict_id,
                        c.scope,
                        c.conflict_type,
                        c.status,
                        c.candidate_claim_ids.len()
                    );
                }
            }
            Ok(())
        }
        CliAction::ConflictResolve(args) => exec_conflict_resolve(*args, token).await,
        CliAction::DocumentList { global, scope } => {
            let client = build_auth_client(&global.url, token)?;
            let resp = client
                .list_documents(scope.as_ref(), global.correlation_id.as_ref())
                .await?;
            if global.json {
                print_json(&resp)?;
            } else {
                println!("documents={}", resp.documents.len());
                for d in &resp.documents {
                    println!(
                        "  document_id={} scope={} key={} version={} title={}",
                        d.document_id, d.scope, d.document_key, d.current_version, d.title
                    );
                }
            }
            Ok(())
        }
        CliAction::DocumentCreate(args) => {
            let DocumentCreateArgs {
                global,
                scope,
                document_key,
                title,
                body,
                provenance_claim_ids,
                provenance_note_ids,
                idempotency_key,
            } = *args;
            let client = build_auth_client(&global.url, token)?;
            let req = CreateDocumentRequest {
                scope,
                document_key,
                title,
                body: resolve_text(body, dennett_contracts::MAX_DOCUMENT_BODY_BYTES)?,
                provenance_claim_ids,
                provenance_note_ids,
                idempotency_key,
            };
            let doc = client
                .create_document(&req, global.correlation_id.as_ref())
                .await?;
            if global.json {
                print_json(&doc)?;
            } else {
                println!(
                    "document_id={} scope={} key={} version={}",
                    doc.document_id, doc.scope, doc.document_key, doc.current_version
                );
            }
            Ok(())
        }
        CliAction::DocumentGet {
            global,
            document_id,
        } => {
            let client = build_auth_client(&global.url, token)?;
            let doc = client
                .get_document(document_id, global.correlation_id.as_ref())
                .await?;
            if global.json {
                print_json(&doc)?;
            } else {
                println!(
                    "document_id={} scope={} key={} version={} title={}\n{}",
                    doc.document_id,
                    doc.scope,
                    doc.document_key,
                    doc.current_version,
                    doc.title,
                    doc.current_body
                );
            }
            Ok(())
        }
        CliAction::DocumentUpdate(args) => exec_document_update(*args, token).await,
        CliAction::DocumentVersions {
            global,
            document_id,
        } => {
            let client = build_auth_client(&global.url, token)?;
            let resp = client
                .get_document_versions(document_id, global.correlation_id.as_ref())
                .await?;
            if global.json {
                print_json(&resp)?;
            } else {
                println!(
                    "document_id={} scope={} current_version={} versions={}",
                    resp.document_id,
                    resp.scope,
                    resp.current_version,
                    resp.versions.len()
                );
                for v in &resp.versions {
                    println!(
                        "  version={} event_id={} diff={}",
                        v.version,
                        v.event_id,
                        v.diff_summary.as_deref().unwrap_or("-")
                    );
                }
            }
            Ok(())
        }
        CliAction::Forget(args) => {
            let ForgetArgs {
                global,
                scope,
                target_type,
                target_id,
                reason,
                idempotency_key,
            } = *args;
            let client = build_auth_client(&global.url, token)?;
            let req = ForgetRequest {
                scope,
                target_type,
                target_id,
                reason,
                idempotency_key,
            };
            let resp = client.forget(&req, global.correlation_id.as_ref()).await?;
            if global.json {
                print_json(&resp)?;
            } else {
                println!(
                    "tombstone_id={} status={} notice={}",
                    resp.tombstone_id, resp.propagation_status, resp.residual_backup_notice
                );
            }
            Ok(())
        }
        CliAction::ForgetTombstones { global, scope } => {
            let client = build_auth_client(&global.url, token)?;
            let resp = client
                .list_tombstones(scope.as_ref(), global.correlation_id.as_ref())
                .await?;
            if global.json {
                print_json(&resp)?;
            } else {
                println!("tombstones={}", resp.tombstones.len());
                for t in &resp.tombstones {
                    println!(
                        "  tombstone_id={} scope={} target={}:{} status={}",
                        t.tombstone_id, t.scope, t.target_type, t.target_id, t.propagation_status
                    );
                }
            }
            Ok(())
        }
        CliAction::Export {
            global,
            scope,
            format_version,
            output,
        } => {
            let client = build_auth_client(&global.url, token)?;
            let resp = client
                .export_package(
                    &ExportRequest {
                        scope,
                        format_version,
                    },
                    global.correlation_id.as_ref(),
                )
                .await?;
            if let Some(ref out_path) = output {
                fs::write(out_path, resp.package_json.as_bytes()).map_err(|err| {
                    DennettError::InvalidInput {
                        field: String::from("--output"),
                        reason: format!(
                            "failed writing export file `{}`: {err}",
                            out_path.display()
                        ),
                    }
                })?;
            }
            if global.json {
                print_json(&resp)?;
            } else if output.is_some() {
                println!(
                    "package_id={} sha256={} items={} bytes={}",
                    resp.package_id, resp.manifest_sha256, resp.item_count, resp.byte_size
                );
            } else {
                println!("{}", resp.package_json);
            }
            Ok(())
        }
        CliAction::Import {
            global,
            scope,
            input,
        } => {
            let client = build_auth_client(&global.url, token)?;
            let package_json = resolve_text(input, MAX_PACKAGE_JSON_BYTES)?;
            let resp = client
                .import_package(
                    &ImportRequest {
                        scope,
                        package_json,
                    },
                    global.correlation_id.as_ref(),
                )
                .await?;
            if global.json {
                print_json(&resp)?;
            } else {
                println!(
                    "import_id={} trust_level={} instruction_active={} imported={} rejected={}",
                    resp.import_id,
                    resp.trust_level,
                    resp.instruction_active,
                    resp.items_imported,
                    resp.items_rejected
                );
            }
            Ok(())
        }
        CliAction::IndexStatus { global, scope } => {
            let client = build_auth_client(&global.url, token)?;
            let state = client
                .get_index_status(scope.as_ref(), global.correlation_id.as_ref())
                .await?;
            if global.json {
                print_json(&state)?;
            } else {
                println!(
                    "scope={} status={} watermark_seq={} model={}",
                    state.scope, state.status, state.watermark_seq, state.model_version
                );
            }
            Ok(())
        }
        CliAction::IndexRebuild { global, scope } => {
            let client = build_auth_client(&global.url, token)?;
            let resp = client
                .trigger_index_rebuild(
                    &TriggerIndexRebuildRequest { scope },
                    global.correlation_id.as_ref(),
                )
                .await?;
            if global.json {
                print_json(&resp)?;
            } else {
                println!(
                    "scope={} status={} watermark_seq={} entries_rebuilt={}",
                    resp.index_state.scope,
                    resp.index_state.status,
                    resp.index_state.watermark_seq,
                    resp.entries_rebuilt
                );
            }
            Ok(())
        }
        CliAction::Stats { global, scope } => {
            let client = build_auth_client(&global.url, token)?;
            let stats = client
                .get_stats(scope.as_ref(), global.correlation_id.as_ref())
                .await?;
            if global.json {
                print_json(&stats)?;
            } else {
                println!(
                    "sources={} notes={} claims={} documents={} open_conflicts={} untrusted={} index_status={}",
                    stats.sources_count,
                    stats.active_notes_count,
                    stats.active_claims_count,
                    stats.documents_count,
                    stats.open_conflicts_count,
                    stats.imported_untrusted_count,
                    stats.index_status
                );
            }
            Ok(())
        }
        CliAction::ConsolidationList { global, scope } => {
            let client = build_auth_client(&global.url, token)?;
            let resp = client
                .list_consolidation_jobs(scope.as_ref(), global.correlation_id.as_ref())
                .await?;
            if global.json {
                print_json(&resp)?;
            } else {
                println!("jobs={}", resp.jobs.len());
                for j in &resp.jobs {
                    println!(
                        "  job_id={} scope={} type={} status={} caveats_preserved={}",
                        j.job_id, j.scope, j.job_type, j.status, j.caveats_preserved
                    );
                }
            }
            Ok(())
        }
        CliAction::ConsolidationCreate {
            global,
            scope,
            job_type,
            idempotency_key,
        } => {
            let client = build_auth_client(&global.url, token)?;
            let job = client
                .create_consolidation_job(
                    &CreateConsolidationJobRequest {
                        scope,
                        job_type,
                        idempotency_key,
                    },
                    global.correlation_id.as_ref(),
                )
                .await?;
            if global.json {
                print_json(&job)?;
            } else {
                println!(
                    "job_id={} scope={} type={} status={} caveats_preserved={}",
                    job.job_id, job.scope, job.job_type, job.status, job.caveats_preserved
                );
            }
            Ok(())
        }
        CliAction::ConsolidationPause { global, job_id } => {
            let client = build_auth_client(&global.url, token)?;
            let job = client
                .pause_consolidation_job(job_id, global.correlation_id.as_ref())
                .await?;
            if global.json {
                print_json(&job)?;
            } else {
                println!("job_id={} status={}", job.job_id, job.status);
            }
            Ok(())
        }
        CliAction::ConsolidationResume { global, job_id } => {
            let client = build_auth_client(&global.url, token)?;
            let job = client
                .resume_consolidation_job(job_id, global.correlation_id.as_ref())
                .await?;
            if global.json {
                print_json(&job)?;
            } else {
                println!("job_id={} status={}", job.job_id, job.status);
            }
            Ok(())
        }
        CliAction::Help | CliAction::Version => Ok(()),
    }
}

async fn exec_claim_create(
    args: ClaimCreateArgs,
    token: Option<String>,
) -> Result<(), DennettError> {
    let client = build_auth_client(&args.global.url, token)?;
    let req = CreateClaimRequest {
        scope: args.scope,
        topic_key: args.topic_key,
        claim_key: args.claim_key,
        natural_language: args.natural_language,
        epistemic_status: args.epistemic_status,
        supersedes_claim_id: None,
        valid_from: args.valid_from,
        valid_to: args.valid_to,
        source_note_id: args.source_note_id,
        caveats: args.caveats,
        provenance: BTreeMap::new(),
        evidence_links: Vec::new(),
        idempotency_key: args.idempotency_key,
    };
    let claim = client
        .create_claim(&req, args.global.correlation_id.as_ref())
        .await?;
    if args.global.json {
        print_json(&claim)?;
    } else {
        println!(
            "claim_id={} scope={} topic={} status={}",
            claim.claim_id, claim.scope, claim.topic_key, claim.epistemic_status
        );
    }
    Ok(())
}

async fn exec_claim_supersede(
    args: ClaimSupersedeArgs,
    token: Option<String>,
) -> Result<(), DennettError> {
    let client = build_auth_client(&args.global.url, token)?;
    let (scope, topic_key, claim_key) = match (args.scope, args.topic_key) {
        (Some(s), Some(t)) => (s, t, args.claim_key),
        (opt_s, opt_t) => {
            let existing = client
                .get_claim(args.claim_id, args.global.correlation_id.as_ref())
                .await?;
            (
                opt_s.unwrap_or(existing.scope),
                opt_t.unwrap_or(existing.topic_key),
                args.claim_key.or(existing.claim_key),
            )
        }
    };
    let req = SupersedeClaimRequest {
        claim_id: args.claim_id,
        scope: scope.clone(),
        new_claim: CreateClaimRequest {
            scope,
            topic_key,
            claim_key,
            natural_language: args.natural_language,
            epistemic_status: args.epistemic_status,
            supersedes_claim_id: Some(args.claim_id),
            valid_from: args.valid_from,
            valid_to: args.valid_to,
            source_note_id: None,
            caveats: args.caveats,
            provenance: BTreeMap::new(),
            evidence_links: Vec::new(),
            idempotency_key: args.idempotency_key,
        },
        resolution_event_id: args.resolution_event_id,
    };
    let claim = client
        .supersede_claim(&req, args.global.correlation_id.as_ref())
        .await?;
    if args.global.json {
        print_json(&claim)?;
    } else {
        println!(
            "claim_id={} supersedes={} status={}",
            claim.claim_id, args.claim_id, claim.epistemic_status
        );
    }
    Ok(())
}

async fn exec_conflict_resolve(
    args: ConflictResolveArgs,
    token: Option<String>,
) -> Result<(), DennettError> {
    let client = build_auth_client(&args.global.url, token)?;
    let scope = if let Some(s) = args.scope {
        s
    } else {
        let list = client
            .list_open_conflicts(None, args.global.correlation_id.as_ref())
            .await?;
        list.conflicts
            .into_iter()
            .find(|c| c.conflict_id == args.conflict_id)
            .map(|c| c.scope)
            .ok_or_else(|| DennettError::NotFound {
                resource: String::from("memory_conflict"),
                id: args.conflict_id.to_string(),
            })?
    };
    let req = ResolveConflictRequest {
        conflict_id: args.conflict_id,
        scope,
        resolution_note: args.resolution_note,
        winning_claim_id: args.winning_claim_id,
        target_status: args.target_status,
        idempotency_key: args.idempotency_key,
    };
    let resolved = client
        .resolve_conflict(&req, args.global.correlation_id.as_ref())
        .await?;
    if args.global.json {
        print_json(&resolved)?;
    } else {
        println!(
            "conflict_id={} status={} resolution_event_id={:?}",
            resolved.conflict_id, resolved.status, resolved.resolution_event_id
        );
    }
    Ok(())
}

async fn exec_document_update(
    args: DocumentUpdateArgs,
    token: Option<String>,
) -> Result<(), DennettError> {
    let client = build_auth_client(&args.global.url, token)?;
    let scope = match args.scope {
        Some(s) => s,
        None => {
            client
                .get_document(args.document_id, args.global.correlation_id.as_ref())
                .await?
                .scope
        }
    };
    let req = UpdateDocumentRequest {
        document_id: args.document_id,
        scope,
        expected_version: args.expected_version,
        title: args.title,
        body: resolve_text(args.body, dennett_contracts::MAX_DOCUMENT_BODY_BYTES)?,
        provenance_claim_ids: args.provenance_claim_ids,
        provenance_note_ids: args.provenance_note_ids,
        diff_summary: args.diff_summary,
        idempotency_key: args.idempotency_key,
    };
    let doc = client
        .update_document(&req, args.global.correlation_id.as_ref())
        .await?;
    if args.global.json {
        print_json(&doc)?;
    } else {
        println!(
            "document_id={} scope={} key={} version={}",
            doc.document_id, doc.scope, doc.document_key, doc.current_version
        );
    }
    Ok(())
}

async fn exec_health(global: &GlobalOptions) -> Result<(), DennettError> {
    let client = MemoryClient::unauthenticated(&global.url)?;
    let health = client.healthz(global.correlation_id.as_ref()).await?;
    if global.json {
        print_json(&health)?;
    } else {
        println!(
            "service={} stage={} memory_fabric={} postgres_connected={} schema_version={:?}",
            health.service,
            health.stage,
            health.memory_fabric_implemented,
            health.postgres_connected,
            health.schema_version
        );
    }
    Ok(())
}

async fn exec_ingest_evidence(
    global: &GlobalOptions,
    token: Option<String>,
    scope: MemoryScope,
    media_type: String,
    content: TextSource,
    source_uri: Option<String>,
    idempotency_key: IdempotencyKey,
) -> Result<(), DennettError> {
    let client = build_auth_client(&global.url, token)?;
    let req = IngestEvidenceRequest {
        scope,
        media_type,
        content: resolve_text(content, MAX_EVIDENCE_CONTENT_BYTES)?,
        source_uri,
        observed_at: None,
        metadata: BTreeMap::new(),
        idempotency_key,
    };
    let resp = client
        .ingest_evidence(&req, global.correlation_id.as_ref())
        .await?;
    if global.json {
        print_json(&resp)?;
    } else {
        println!(
            "evidence_id={} event_id={} sha256={} replay={}",
            resp.evidence.evidence_id, resp.event_id, resp.evidence.sha256, resp.idempotent_replay
        );
    }
    Ok(())
}

async fn exec_write(
    global: &GlobalOptions,
    token: Option<String>,
    req: CreateNoteRequest,
) -> Result<(), DennettError> {
    let client = build_auth_client(&global.url, token)?;
    let resp = client
        .create_note(&req, global.correlation_id.as_ref())
        .await?;
    if global.json {
        print_json(&resp)?;
    } else {
        println!(
            "note_id={} revision={} event_id={} scope={} replay={}",
            resp.note.note_id,
            resp.note.current_revision,
            resp.event_id,
            resp.note.scope,
            resp.idempotent_replay
        );
    }
    Ok(())
}

async fn exec_get(
    global: &GlobalOptions,
    token: Option<String>,
    note_id: MemoryNoteId,
) -> Result<(), DennettError> {
    let client = build_auth_client(&global.url, token)?;
    let note = client
        .get_note_current(note_id, global.correlation_id.as_ref())
        .await?;
    if global.json {
        print_json(&note)?;
    } else {
        println!(
            "note_id={} scope={} rev={} title={}\n{}",
            note.note_id, note.scope, note.current_revision, note.title, note.body
        );
    }
    Ok(())
}

async fn exec_history(
    global: &GlobalOptions,
    token: Option<String>,
    note_id: MemoryNoteId,
) -> Result<(), DennettError> {
    let client = build_auth_client(&global.url, token)?;
    let history = client
        .get_note_history(note_id, global.correlation_id.as_ref())
        .await?;
    if global.json {
        print_json(&history)?;
    } else {
        println!(
            "note_id={} scope={} current_revision={} revisions={}",
            history.note_id,
            history.scope,
            history.current_revision,
            history.revisions.len()
        );
        for rev in &history.revisions {
            println!(
                "  rev={} recorded_by={} title={} rationale={}",
                rev.revision,
                rev.recorded_by,
                rev.title,
                rev.rationale.as_deref().unwrap_or("-")
            );
        }
    }
    Ok(())
}

async fn exec_correct(
    global: &GlobalOptions,
    token: Option<String>,
    req: CorrectNoteRequest,
) -> Result<(), DennettError> {
    let client = build_auth_client(&global.url, token)?;
    let resp = client
        .correct_note(&req, global.correlation_id.as_ref())
        .await?;
    if global.json {
        print_json(&resp)?;
    } else {
        println!(
            "note_id={} revision={} event_id={} replay={}",
            resp.note.note_id, resp.note.current_revision, resp.event_id, resp.idempotent_replay
        );
    }
    Ok(())
}

async fn exec_search(
    global: &GlobalOptions,
    token: Option<String>,
    req: SearchNotesRequest,
) -> Result<(), DennettError> {
    let client = build_auth_client(&global.url, token)?;
    let resp = client
        .search_notes(&req, global.correlation_id.as_ref())
        .await?;
    if global.json {
        print_json(&resp)?;
    } else {
        println!(
            "matched={} returned={} truncated={} index_status={}",
            resp.total_matched,
            resp.notes.len(),
            resp.truncated,
            resp.index_status
        );
        for note in &resp.notes {
            println!(
                "  note_id={} scope={} rev={} title={}",
                note.note_id, note.scope, note.current_revision, note.title
            );
        }
    }
    Ok(())
}

async fn exec_event(
    global: &GlobalOptions,
    token: Option<String>,
    event_id: MemoryEventId,
) -> Result<(), DennettError> {
    let client = build_auth_client(&global.url, token)?;
    let event = client
        .get_event(event_id, global.correlation_id.as_ref())
        .await?;
    if global.json {
        print_json(&event)?;
    } else {
        println!(
            "event_id={} kind={} scope={} seq={} links={}",
            event.event_id,
            event.kind,
            event.scope,
            event.stream_seq,
            event.evidence_links.len()
        );
    }
    Ok(())
}

fn build_auth_client(url: &str, token: Option<String>) -> Result<MemoryClient, DennettError> {
    let Some(tok) = token else {
        return Err(DennettError::Unauthorized {
            reason: format!(
                "{ENV_MEMORY_TOKEN} environment variable is required (tokens are never accepted via CLI flags)"
            ),
        });
    };
    MemoryClient::new(url, tok)
}

fn resolve_text(source: TextSource, max_bytes: usize) -> Result<String, DennettError> {
    match source {
        TextSource::Inline(s) => Ok(s),
        TextSource::Stdin => {
            let mut buf = Vec::new();
            let mut limited = io::stdin().lock().take((max_bytes + 1) as u64);
            limited
                .read_to_end(&mut buf)
                .map_err(|err| DennettError::InvalidInput {
                    field: String::from("stdin"),
                    reason: format!("failed to read stdin: {err}"),
                })?;
            if buf.len() > max_bytes {
                return Err(DennettError::InvalidInput {
                    field: String::from("stdin"),
                    reason: format!("stdin content exceeds maximum limit of {max_bytes} bytes"),
                });
            }
            String::from_utf8(buf).map_err(|_| DennettError::InvalidInput {
                field: String::from("stdin"),
                reason: String::from("stdin must be valid UTF-8"),
            })
        }
        TextSource::File(path) => {
            let bytes = fs::read(&path).map_err(|err| DennettError::InvalidInput {
                field: String::from("--input"),
                reason: format!("failed to read `{}`: {err}", path.display()),
            })?;
            if bytes.len() > max_bytes {
                return Err(DennettError::InvalidInput {
                    field: String::from("--input"),
                    reason: format!("file content exceeds maximum limit of {max_bytes} bytes"),
                });
            }
            String::from_utf8(bytes).map_err(|_| DennettError::InvalidInput {
                field: String::from("--input"),
                reason: String::from("input file must be valid UTF-8"),
            })
        }
    }
}

fn parse_cli<I>(args: I, default_url: &str) -> Result<CliAction, DennettError>
where
    I: IntoIterator<Item = String>,
{
    let mut iter = args.into_iter().peekable();
    let mut global = GlobalOptions {
        url: default_url.to_owned(),
        json: false,
        correlation_id: None,
    };

    let mut subcommand: Option<String> = None;
    let mut rest: Vec<String> = Vec::new();

    while let Some(arg) = iter.next() {
        if subcommand.is_none() {
            match arg.as_str() {
                "-h" | "--help" => return Ok(CliAction::Help),
                "-V" | "--version" => return Ok(CliAction::Version),
                "--json" => global.json = true,
                "--url" => {
                    global.url = next_val(&mut iter, "--url")?;
                }
                "--correlation-id" => {
                    let raw = next_val(&mut iter, "--correlation-id")?;
                    global.correlation_id = Some(CorrelationId::try_new(raw)?);
                }
                flag if flag.starts_with('-') => {
                    return Err(reject_unknown_flag(flag));
                }
                cmd => {
                    subcommand = Some(cmd.to_owned());
                }
            }
        } else {
            rest.push(arg);
        }
    }

    let Some(cmd) = subcommand else {
        return Err(DennettError::InvalidInput {
            field: String::from("cli"),
            reason: String::from("missing subcommand (run with --help)"),
        });
    };

    if rest.iter().any(|a| a == "-h" || a == "--help") {
        return Ok(CliAction::Help);
    }

    match cmd.as_str() {
        "health" => {
            let mut it = rest.into_iter();
            while let Some(arg) = it.next() {
                if !consume_global_flag(&arg, &mut it, &mut global)? {
                    return Err(reject_unknown_flag(&arg));
                }
            }
            Ok(CliAction::Health { global })
        }
        "ingest-evidence" => parse_ingest_evidence(rest, global),
        "write" => parse_write(rest, global),
        "get" => {
            let note_id = parse_single_id::<MemoryNoteId>(rest, &mut global, "note_id")?;
            Ok(CliAction::Get { global, note_id })
        }
        "history" => {
            let note_id = parse_single_id::<MemoryNoteId>(rest, &mut global, "note_id")?;
            Ok(CliAction::History { global, note_id })
        }
        "correct" => parse_correct(rest, global),
        "search" => parse_search(rest, global),
        "event" => {
            let event_id = parse_single_id::<MemoryEventId>(rest, &mut global, "event_id")?;
            Ok(CliAction::Event { global, event_id })
        }
        "claim" => parse_claim_subcommand(rest, global),
        "conflict" => parse_conflict_subcommand(rest, global),
        "document" => parse_document_subcommand(rest, global),
        "forget" => parse_forget_subcommand(rest, global),
        "export" => parse_export(rest, global),
        "import" => parse_import(rest, global),
        "index" => parse_index_subcommand(rest, global),
        "stats" => {
            let scope = parse_optional_scope_only(rest, &mut global)?;
            Ok(CliAction::Stats { global, scope })
        }
        "consolidation" => parse_consolidation_subcommand(rest, global),
        other => Err(DennettError::InvalidInput {
            field: String::from("subcommand"),
            reason: format!("unknown subcommand `{}`", sanitize_cli_arg(other)),
        }),
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "single parser handles all claim subcommands (create/get/supersede/list) uniformly"
)]
fn parse_claim_subcommand(
    args: Vec<String>,
    mut global: GlobalOptions,
) -> Result<CliAction, DennettError> {
    let mut it = args.into_iter();
    let Some(sub) = it.next() else {
        return Err(DennettError::InvalidInput {
            field: String::from("claim"),
            reason: String::from("expected claim subcommand: create | get | supersede | list"),
        });
    };
    let tail: Vec<String> = it.collect();
    match sub.as_str() {
        "create" => {
            let mut iter = tail.into_iter();
            let mut scope: Option<MemoryScope> = None;
            let mut topic_key: Option<String> = None;
            let mut claim_key: Option<String> = None;
            let mut text: Option<String> = None;
            let mut epistemic_status = EpistemicStatus::Asserted;
            let mut caveats = Vec::new();
            let mut valid_from: Option<TimestampMs> = None;
            let mut valid_to: Option<TimestampMs> = None;
            let mut source_note_id: Option<MemoryNoteId> = None;
            let mut idempotency_key: Option<IdempotencyKey> = None;
            while let Some(arg) = iter.next() {
                if consume_global_flag(&arg, &mut iter, &mut global)? {
                    continue;
                }
                match arg.as_str() {
                    "--scope" => {
                        scope = Some(MemoryScope::try_new(next_val(&mut iter, "--scope")?)?);
                    }
                    "--topic-key" => topic_key = Some(next_val(&mut iter, "--topic-key")?),
                    "--claim-key" => claim_key = Some(next_val(&mut iter, "--claim-key")?),
                    "--text" => text = Some(next_val(&mut iter, "--text")?),
                    "--status" => epistemic_status = next_val(&mut iter, "--status")?.parse()?,
                    "--caveat" => caveats.push(next_val(&mut iter, "--caveat")?),
                    "--valid-from" => {
                        valid_from = Some(parse_timestamp_arg(
                            &next_val(&mut iter, "--valid-from")?,
                            "--valid-from",
                        )?);
                    }
                    "--valid-to" => {
                        valid_to = Some(parse_timestamp_arg(
                            &next_val(&mut iter, "--valid-to")?,
                            "--valid-to",
                        )?);
                    }
                    "--source-note" => {
                        source_note_id = Some(MemoryNoteId::from_str(&next_val(
                            &mut iter,
                            "--source-note",
                        )?)?);
                    }
                    "--idempotency-key" => {
                        idempotency_key = Some(IdempotencyKey::try_new(next_val(
                            &mut iter,
                            "--idempotency-key",
                        )?)?);
                    }
                    other => return Err(reject_unknown_flag(other)),
                }
            }
            Ok(CliAction::ClaimCreate(Box::new(ClaimCreateArgs {
                global,
                scope: require_field(scope, "--scope")?,
                topic_key: require_field(topic_key, "--topic-key")?,
                claim_key,
                natural_language: require_field(text, "--text")?,
                epistemic_status,
                caveats,
                valid_from,
                valid_to,
                source_note_id,
                idempotency_key: idempotency_key.unwrap_or_else(IdempotencyKey::generate),
            })))
        }
        "get" => {
            let claim_id = parse_single_id::<ClaimId>(tail, &mut global, "claim_id")?;
            Ok(CliAction::ClaimGet { global, claim_id })
        }
        "supersede" => {
            let mut iter = tail.into_iter();
            let mut claim_id: Option<ClaimId> = None;
            let mut scope: Option<MemoryScope> = None;
            let mut topic_key: Option<String> = None;
            let mut claim_key: Option<String> = None;
            let mut text: Option<String> = None;
            let mut epistemic_status = EpistemicStatus::Asserted;
            let mut caveats = Vec::new();
            let mut valid_from: Option<TimestampMs> = None;
            let mut valid_to: Option<TimestampMs> = None;
            let mut resolution_event_id: Option<MemoryEventId> = None;
            let mut idempotency_key: Option<IdempotencyKey> = None;
            while let Some(arg) = iter.next() {
                if consume_global_flag(&arg, &mut iter, &mut global)? {
                    continue;
                }
                match arg.as_str() {
                    "--id" => claim_id = Some(ClaimId::from_str(&next_val(&mut iter, "--id")?)?),
                    "--scope" => {
                        scope = Some(MemoryScope::try_new(next_val(&mut iter, "--scope")?)?);
                    }
                    "--topic-key" => topic_key = Some(next_val(&mut iter, "--topic-key")?),
                    "--claim-key" => claim_key = Some(next_val(&mut iter, "--claim-key")?),
                    "--text" => text = Some(next_val(&mut iter, "--text")?),
                    "--status" => epistemic_status = next_val(&mut iter, "--status")?.parse()?,
                    "--caveat" => caveats.push(next_val(&mut iter, "--caveat")?),
                    "--valid-from" => {
                        valid_from = Some(parse_timestamp_arg(
                            &next_val(&mut iter, "--valid-from")?,
                            "--valid-from",
                        )?);
                    }
                    "--valid-to" => {
                        valid_to = Some(parse_timestamp_arg(
                            &next_val(&mut iter, "--valid-to")?,
                            "--valid-to",
                        )?);
                    }
                    "--resolution-event" => {
                        resolution_event_id = Some(MemoryEventId::from_str(&next_val(
                            &mut iter,
                            "--resolution-event",
                        )?)?);
                    }
                    "--idempotency-key" => {
                        idempotency_key = Some(IdempotencyKey::try_new(next_val(
                            &mut iter,
                            "--idempotency-key",
                        )?)?);
                    }
                    pos if !pos.starts_with('-') && claim_id.is_none() => {
                        claim_id = Some(ClaimId::from_str(pos)?);
                    }
                    other => return Err(reject_unknown_flag(other)),
                }
            }
            Ok(CliAction::ClaimSupersede(Box::new(ClaimSupersedeArgs {
                global,
                claim_id: require_field(claim_id, "<claim_id>")?,
                scope,
                topic_key,
                claim_key,
                natural_language: require_field(text, "--text")?,
                epistemic_status,
                caveats,
                valid_from,
                valid_to,
                resolution_event_id,
                idempotency_key: idempotency_key.unwrap_or_else(IdempotencyKey::generate),
            })))
        }
        "list" => {
            let mut iter = tail.into_iter();
            let mut scope: Option<MemoryScope> = None;
            let mut topic_key: Option<String> = None;
            while let Some(arg) = iter.next() {
                if consume_global_flag(&arg, &mut iter, &mut global)? {
                    continue;
                }
                match arg.as_str() {
                    "--scope" => {
                        scope = Some(MemoryScope::try_new(next_val(&mut iter, "--scope")?)?);
                    }
                    "--topic-key" => topic_key = Some(next_val(&mut iter, "--topic-key")?),
                    other => return Err(reject_unknown_flag(other)),
                }
            }
            Ok(CliAction::ClaimList {
                global,
                scope,
                topic_key,
            })
        }
        other => Err(DennettError::InvalidInput {
            field: String::from("claim"),
            reason: format!("unknown claim subcommand `{}`", sanitize_cli_arg(other)),
        }),
    }
}

fn parse_conflict_subcommand(
    args: Vec<String>,
    mut global: GlobalOptions,
) -> Result<CliAction, DennettError> {
    let mut it = args.into_iter();
    let Some(sub) = it.next() else {
        return Err(DennettError::InvalidInput {
            field: String::from("conflict"),
            reason: String::from("expected conflict subcommand: list | resolve"),
        });
    };
    let tail: Vec<String> = it.collect();
    match sub.as_str() {
        "list" => {
            let scope = parse_optional_scope_only(tail, &mut global)?;
            Ok(CliAction::ConflictList { global, scope })
        }
        "resolve" => {
            let mut iter = tail.into_iter();
            let mut conflict_id: Option<ConflictId> = None;
            let mut scope: Option<MemoryScope> = None;
            let mut winning_claim_id: Option<ClaimId> = None;
            let mut target_status: Option<ConflictStatus> = None;
            let mut resolution_note: Option<String> = None;
            let mut idempotency_key: Option<IdempotencyKey> = None;
            while let Some(arg) = iter.next() {
                if consume_global_flag(&arg, &mut iter, &mut global)? {
                    continue;
                }
                match arg.as_str() {
                    "--id" => {
                        conflict_id = Some(ConflictId::from_str(&next_val(&mut iter, "--id")?)?);
                    }
                    "--scope" => {
                        scope = Some(MemoryScope::try_new(next_val(&mut iter, "--scope")?)?);
                    }
                    "--winning-claim" => {
                        winning_claim_id =
                            Some(ClaimId::from_str(&next_val(&mut iter, "--winning-claim")?)?);
                    }
                    "--status" => target_status = Some(next_val(&mut iter, "--status")?.parse()?),
                    "--note" => resolution_note = Some(next_val(&mut iter, "--note")?),
                    "--idempotency-key" => {
                        idempotency_key = Some(IdempotencyKey::try_new(next_val(
                            &mut iter,
                            "--idempotency-key",
                        )?)?);
                    }
                    pos if !pos.starts_with('-') && conflict_id.is_none() => {
                        conflict_id = Some(ConflictId::from_str(pos)?);
                    }
                    other => return Err(reject_unknown_flag(other)),
                }
            }
            Ok(CliAction::ConflictResolve(Box::new(ConflictResolveArgs {
                global,
                conflict_id: require_field(conflict_id, "<conflict_id>")?,
                scope,
                winning_claim_id,
                target_status,
                resolution_note: require_field(resolution_note, "--note")?,
                idempotency_key: idempotency_key.unwrap_or_else(IdempotencyKey::generate),
            })))
        }
        other => Err(DennettError::InvalidInput {
            field: String::from("conflict"),
            reason: format!("unknown conflict subcommand `{}`", sanitize_cli_arg(other)),
        }),
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "single parser handles all document subcommands (list/create/get/update/versions) uniformly"
)]
fn parse_document_subcommand(
    args: Vec<String>,
    mut global: GlobalOptions,
) -> Result<CliAction, DennettError> {
    let mut it = args.into_iter();
    let Some(sub) = it.next() else {
        return Err(DennettError::InvalidInput {
            field: String::from("document"),
            reason: String::from(
                "expected document subcommand: list | create | get | update | versions",
            ),
        });
    };
    let tail: Vec<String> = it.collect();
    match sub.as_str() {
        "list" => {
            let scope = parse_optional_scope_only(tail, &mut global)?;
            Ok(CliAction::DocumentList { global, scope })
        }
        "create" => {
            let mut iter = tail.into_iter();
            let mut scope: Option<MemoryScope> = None;
            let mut key: Option<String> = None;
            let mut title: Option<String> = None;
            let mut body: Option<TextSource> = None;
            let mut claim_ids = Vec::new();
            let mut note_ids = Vec::new();
            let mut idempotency_key: Option<IdempotencyKey> = None;
            while let Some(arg) = iter.next() {
                if consume_global_flag(&arg, &mut iter, &mut global)? {
                    continue;
                }
                match arg.as_str() {
                    "--scope" => {
                        scope = Some(MemoryScope::try_new(next_val(&mut iter, "--scope")?)?);
                    }
                    "--key" => key = Some(next_val(&mut iter, "--key")?),
                    "--title" => title = Some(next_val(&mut iter, "--title")?),
                    "--body" => body = Some(TextSource::Inline(next_val(&mut iter, "--body")?)),
                    "--stdin" => body = Some(TextSource::Stdin),
                    "--claim-id" => {
                        claim_ids.push(ClaimId::from_str(&next_val(&mut iter, "--claim-id")?)?);
                    }
                    "--note-id" => {
                        note_ids.push(MemoryNoteId::from_str(&next_val(&mut iter, "--note-id")?)?);
                    }
                    "--idempotency-key" => {
                        idempotency_key = Some(IdempotencyKey::try_new(next_val(
                            &mut iter,
                            "--idempotency-key",
                        )?)?);
                    }
                    other => return Err(reject_unknown_flag(other)),
                }
            }
            Ok(CliAction::DocumentCreate(Box::new(DocumentCreateArgs {
                global,
                scope: require_field(scope, "--scope")?,
                document_key: require_field(key, "--key")?,
                title: require_field(title, "--title")?,
                body: require_field(body, "--body or --stdin")?,
                provenance_claim_ids: claim_ids,
                provenance_note_ids: note_ids,
                idempotency_key: idempotency_key.unwrap_or_else(IdempotencyKey::generate),
            })))
        }
        "get" => {
            let document_id = parse_single_id::<DocumentId>(tail, &mut global, "document_id")?;
            Ok(CliAction::DocumentGet {
                global,
                document_id,
            })
        }
        "update" => {
            let mut iter = tail.into_iter();
            let mut document_id: Option<DocumentId> = None;
            let mut scope: Option<MemoryScope> = None;
            let mut expected_version: Option<i32> = None;
            let mut title: Option<String> = None;
            let mut body: Option<TextSource> = None;
            let mut claim_ids = Vec::new();
            let mut note_ids = Vec::new();
            let mut diff_summary: Option<String> = None;
            let mut idempotency_key: Option<IdempotencyKey> = None;
            while let Some(arg) = iter.next() {
                if consume_global_flag(&arg, &mut iter, &mut global)? {
                    continue;
                }
                match arg.as_str() {
                    "--id" => {
                        document_id = Some(DocumentId::from_str(&next_val(&mut iter, "--id")?)?);
                    }
                    "--scope" => {
                        scope = Some(MemoryScope::try_new(next_val(&mut iter, "--scope")?)?);
                    }
                    "--expected-version" => {
                        let raw = next_val(&mut iter, "--expected-version")?;
                        expected_version =
                            Some(raw.parse::<i32>().map_err(|_| DennettError::InvalidInput {
                                field: String::from("--expected-version"),
                                reason: String::from("must be a positive integer"),
                            })?);
                    }
                    "--title" => title = Some(next_val(&mut iter, "--title")?),
                    "--body" => body = Some(TextSource::Inline(next_val(&mut iter, "--body")?)),
                    "--stdin" => body = Some(TextSource::Stdin),
                    "--claim-id" => {
                        claim_ids.push(ClaimId::from_str(&next_val(&mut iter, "--claim-id")?)?);
                    }
                    "--note-id" => {
                        note_ids.push(MemoryNoteId::from_str(&next_val(&mut iter, "--note-id")?)?);
                    }
                    "--diff-summary" => diff_summary = Some(next_val(&mut iter, "--diff-summary")?),
                    "--idempotency-key" => {
                        idempotency_key = Some(IdempotencyKey::try_new(next_val(
                            &mut iter,
                            "--idempotency-key",
                        )?)?);
                    }
                    pos if !pos.starts_with('-') && document_id.is_none() => {
                        document_id = Some(DocumentId::from_str(pos)?);
                    }
                    other => return Err(reject_unknown_flag(other)),
                }
            }
            Ok(CliAction::DocumentUpdate(Box::new(DocumentUpdateArgs {
                global,
                document_id: require_field(document_id, "<document_id>")?,
                scope,
                expected_version: require_field(expected_version, "--expected-version")?,
                title: require_field(title, "--title")?,
                body: require_field(body, "--body or --stdin")?,
                provenance_claim_ids: claim_ids,
                provenance_note_ids: note_ids,
                diff_summary,
                idempotency_key: idempotency_key.unwrap_or_else(IdempotencyKey::generate),
            })))
        }
        "versions" => {
            let document_id = parse_single_id::<DocumentId>(tail, &mut global, "document_id")?;
            Ok(CliAction::DocumentVersions {
                global,
                document_id,
            })
        }
        other => Err(DennettError::InvalidInput {
            field: String::from("document"),
            reason: format!("unknown document subcommand `{}`", sanitize_cli_arg(other)),
        }),
    }
}

fn parse_forget_subcommand(
    args: Vec<String>,
    mut global: GlobalOptions,
) -> Result<CliAction, DennettError> {
    if args.first().map(String::as_str) == Some("tombstones") {
        let scope = parse_optional_scope_only(args.into_iter().skip(1).collect(), &mut global)?;
        return Ok(CliAction::ForgetTombstones { global, scope });
    }
    let mut iter = args.into_iter();
    let mut scope: Option<MemoryScope> = None;
    let mut target_type: Option<ForgetTargetType> = None;
    let mut target_id: Option<Uuid> = None;
    let mut reason: Option<String> = None;
    let mut idempotency_key: Option<IdempotencyKey> = None;
    while let Some(arg) = iter.next() {
        if consume_global_flag(&arg, &mut iter, &mut global)? {
            continue;
        }
        match arg.as_str() {
            "--scope" => scope = Some(MemoryScope::try_new(next_val(&mut iter, "--scope")?)?),
            "--target-type" => target_type = Some(next_val(&mut iter, "--target-type")?.parse()?),
            "--target-id" => {
                let raw = next_val(&mut iter, "--target-id")?;
                target_id =
                    Some(
                        Uuid::parse_str(&raw).map_err(|_| DennettError::InvalidInput {
                            field: String::from("--target-id"),
                            reason: String::from("must be a valid UUID"),
                        })?,
                    );
            }
            "--reason" => reason = Some(next_val(&mut iter, "--reason")?),
            "--idempotency-key" => {
                idempotency_key = Some(IdempotencyKey::try_new(next_val(
                    &mut iter,
                    "--idempotency-key",
                )?)?);
            }
            other => return Err(reject_unknown_flag(other)),
        }
    }
    Ok(CliAction::Forget(Box::new(ForgetArgs {
        global,
        scope: require_field(scope, "--scope")?,
        target_type: require_field(target_type, "--target-type")?,
        target_id: require_field(target_id, "--target-id")?,
        reason: require_field(reason, "--reason")?,
        idempotency_key: idempotency_key.unwrap_or_else(IdempotencyKey::generate),
    })))
}

fn parse_export(args: Vec<String>, mut global: GlobalOptions) -> Result<CliAction, DennettError> {
    let mut iter = args.into_iter();
    let mut scope: Option<MemoryScope> = None;
    let mut format_version: Option<String> = None;
    let mut output: Option<PathBuf> = None;
    while let Some(arg) = iter.next() {
        if consume_global_flag(&arg, &mut iter, &mut global)? {
            continue;
        }
        match arg.as_str() {
            "--scope" => scope = Some(MemoryScope::try_new(next_val(&mut iter, "--scope")?)?),
            "--format-version" => format_version = Some(next_val(&mut iter, "--format-version")?),
            "--output" => output = Some(PathBuf::from(next_val(&mut iter, "--output")?)),
            other => return Err(reject_unknown_flag(other)),
        }
    }
    Ok(CliAction::Export {
        global,
        scope: require_field(scope, "--scope")?,
        format_version,
        output,
    })
}

fn parse_import(args: Vec<String>, mut global: GlobalOptions) -> Result<CliAction, DennettError> {
    let mut iter = args.into_iter();
    let mut scope: Option<MemoryScope> = None;
    let mut input: Option<TextSource> = None;
    while let Some(arg) = iter.next() {
        if consume_global_flag(&arg, &mut iter, &mut global)? {
            continue;
        }
        match arg.as_str() {
            "--scope" => scope = Some(MemoryScope::try_new(next_val(&mut iter, "--scope")?)?),
            "--input" => {
                input = Some(TextSource::File(PathBuf::from(next_val(
                    &mut iter, "--input",
                )?)));
            }
            "--stdin" => input = Some(TextSource::Stdin),
            other => return Err(reject_unknown_flag(other)),
        }
    }
    Ok(CliAction::Import {
        global,
        scope: require_field(scope, "--scope")?,
        input: require_field(input, "--input or --stdin")?,
    })
}

fn parse_index_subcommand(
    args: Vec<String>,
    mut global: GlobalOptions,
) -> Result<CliAction, DennettError> {
    let mut it = args.into_iter();
    let Some(sub) = it.next() else {
        return Err(DennettError::InvalidInput {
            field: String::from("index"),
            reason: String::from("expected index subcommand: status | rebuild"),
        });
    };
    let tail: Vec<String> = it.collect();
    match sub.as_str() {
        "status" => {
            let scope = parse_optional_scope_only(tail, &mut global)?;
            Ok(CliAction::IndexStatus { global, scope })
        }
        "rebuild" => {
            let scope = parse_optional_scope_only(tail, &mut global)?;
            Ok(CliAction::IndexRebuild {
                global,
                scope: require_field(scope, "--scope")?,
            })
        }
        other => Err(DennettError::InvalidInput {
            field: String::from("index"),
            reason: format!("unknown index subcommand `{}`", sanitize_cli_arg(other)),
        }),
    }
}

fn parse_consolidation_subcommand(
    args: Vec<String>,
    mut global: GlobalOptions,
) -> Result<CliAction, DennettError> {
    let mut it = args.into_iter();
    let Some(sub) = it.next() else {
        return Err(DennettError::InvalidInput {
            field: String::from("consolidation"),
            reason: String::from(
                "expected consolidation subcommand: list | create | pause | resume",
            ),
        });
    };
    let tail: Vec<String> = it.collect();
    match sub.as_str() {
        "list" => {
            let scope = parse_optional_scope_only(tail, &mut global)?;
            Ok(CliAction::ConsolidationList { global, scope })
        }
        "create" => {
            let mut iter = tail.into_iter();
            let mut scope: Option<MemoryScope> = None;
            let mut job_type: Option<ConsolidationJobType> = None;
            let mut idempotency_key: Option<IdempotencyKey> = None;
            while let Some(arg) = iter.next() {
                if consume_global_flag(&arg, &mut iter, &mut global)? {
                    continue;
                }
                match arg.as_str() {
                    "--scope" => {
                        scope = Some(MemoryScope::try_new(next_val(&mut iter, "--scope")?)?);
                    }
                    "--type" => job_type = Some(next_val(&mut iter, "--type")?.parse()?),
                    "--idempotency-key" => {
                        idempotency_key = Some(IdempotencyKey::try_new(next_val(
                            &mut iter,
                            "--idempotency-key",
                        )?)?);
                    }
                    other => return Err(reject_unknown_flag(other)),
                }
            }
            Ok(CliAction::ConsolidationCreate {
                global,
                scope: require_field(scope, "--scope")?,
                job_type: require_field(job_type, "--type")?,
                idempotency_key,
            })
        }
        "pause" => {
            let job_id = parse_single_id::<ConsolidationJobId>(tail, &mut global, "job_id")?;
            Ok(CliAction::ConsolidationPause { global, job_id })
        }
        "resume" => {
            let job_id = parse_single_id::<ConsolidationJobId>(tail, &mut global, "job_id")?;
            Ok(CliAction::ConsolidationResume { global, job_id })
        }
        other => Err(DennettError::InvalidInput {
            field: String::from("consolidation"),
            reason: format!(
                "unknown consolidation subcommand `{}`",
                sanitize_cli_arg(other)
            ),
        }),
    }
}

fn parse_optional_scope_only(
    args: Vec<String>,
    global: &mut GlobalOptions,
) -> Result<Option<MemoryScope>, DennettError> {
    let mut iter = args.into_iter();
    let mut scope: Option<MemoryScope> = None;
    while let Some(arg) = iter.next() {
        if consume_global_flag(&arg, &mut iter, global)? {
            continue;
        }
        match arg.as_str() {
            "--scope" => scope = Some(MemoryScope::try_new(next_val(&mut iter, "--scope")?)?),
            other => return Err(reject_unknown_flag(other)),
        }
    }
    Ok(scope)
}

fn parse_timestamp_arg(raw: &str, flag: &str) -> Result<TimestampMs, DennettError> {
    let ms = raw.parse::<i64>().map_err(|_| DennettError::InvalidInput {
        field: flag.to_owned(),
        reason: String::from("must be UTC epoch milliseconds"),
    })?;
    let ts = TimestampMs::from_millis(ms);
    ts.validate(flag)?;
    Ok(ts)
}

fn parse_ingest_evidence(
    args: Vec<String>,
    mut global: GlobalOptions,
) -> Result<CliAction, DennettError> {
    let mut it = args.into_iter();
    let mut scope: Option<MemoryScope> = None;
    let mut media_type = String::from("text/plain");
    let mut content: Option<TextSource> = None;
    let mut source_uri: Option<String> = None;
    let mut idempotency_key: Option<IdempotencyKey> = None;

    while let Some(arg) = it.next() {
        if consume_global_flag(&arg, &mut it, &mut global)? {
            continue;
        }
        match arg.as_str() {
            "--scope" => scope = Some(MemoryScope::try_new(next_val(&mut it, "--scope")?)?),
            "--media-type" => media_type = next_val(&mut it, "--media-type")?,
            "--content" => content = Some(TextSource::Inline(next_val(&mut it, "--content")?)),
            "--stdin" => content = Some(TextSource::Stdin),
            "--source-uri" => source_uri = Some(next_val(&mut it, "--source-uri")?),
            "--idempotency-key" => {
                idempotency_key = Some(IdempotencyKey::try_new(next_val(
                    &mut it,
                    "--idempotency-key",
                )?)?);
            }
            other => return Err(reject_unknown_flag(other)),
        }
    }

    Ok(CliAction::IngestEvidence {
        global,
        scope: require_field(scope, "--scope")?,
        media_type,
        content: require_field(content, "--content or --stdin")?,
        source_uri,
        idempotency_key: idempotency_key.unwrap_or_else(IdempotencyKey::generate),
    })
}

fn parse_write(args: Vec<String>, mut global: GlobalOptions) -> Result<CliAction, DennettError> {
    let mut it = args.into_iter();
    let mut scope: Option<MemoryScope> = None;
    let mut title: Option<String> = None;
    let mut body: Option<TextSource> = None;
    let mut tags: Vec<String> = Vec::new();
    let mut evidence_links: Vec<EvidenceLinkInput> = Vec::new();
    let mut agent_label: Option<String> = None;
    let mut admission_status: Option<AdmissionStatus> = None;
    let mut salience: Option<u8> = None;
    let mut topic_key: Option<String> = None;
    let mut idempotency_key: Option<IdempotencyKey> = None;

    while let Some(arg) = it.next() {
        if consume_global_flag(&arg, &mut it, &mut global)? {
            continue;
        }
        match arg.as_str() {
            "--scope" => scope = Some(MemoryScope::try_new(next_val(&mut it, "--scope")?)?),
            "--title" => title = Some(next_val(&mut it, "--title")?),
            "--body" => body = Some(TextSource::Inline(next_val(&mut it, "--body")?)),
            "--stdin" => body = Some(TextSource::Stdin),
            "--tag" => tags.push(next_val(&mut it, "--tag")?),
            "--evidence-id" => {
                let id = EvidenceId::from_str(&next_val(&mut it, "--evidence-id")?)?;
                evidence_links.push(EvidenceLinkInput {
                    evidence_id: id,
                    relationship: EvidenceRelationship::try_new("supports")?,
                    anchor: None,
                    ordinal: None,
                });
            }
            "--evidence-link-json" => {
                let raw = next_val(&mut it, "--evidence-link-json")?;
                let parsed: EvidenceLinkInput =
                    serde_json::from_str(&raw).map_err(|err| DennettError::InvalidInput {
                        field: String::from("--evidence-link-json"),
                        reason: format!("invalid JSON link: {err}"),
                    })?;
                evidence_links.push(parsed);
            }
            "--agent" => agent_label = Some(next_val(&mut it, "--agent")?),
            "--admission-status" => {
                admission_status = Some(next_val(&mut it, "--admission-status")?.parse()?);
            }
            "--salience" => {
                let raw = next_val(&mut it, "--salience")?;
                salience = Some(raw.parse::<u8>().map_err(|_| DennettError::InvalidInput {
                    field: String::from("--salience"),
                    reason: String::from("must be an integer between 0 and 100"),
                })?);
            }
            "--topic-key" => topic_key = Some(next_val(&mut it, "--topic-key")?),
            "--idempotency-key" => {
                idempotency_key = Some(IdempotencyKey::try_new(next_val(
                    &mut it,
                    "--idempotency-key",
                )?)?);
            }
            other => return Err(reject_unknown_flag(other)),
        }
    }

    Ok(CliAction::Write(Box::new(WriteArgs {
        global,
        scope: require_field(scope, "--scope")?,
        title: require_field(title, "--title")?,
        body: require_field(body, "--body or --stdin")?,
        tags,
        evidence_links,
        agent_label,
        admission_status,
        salience,
        topic_key,
        idempotency_key: idempotency_key.unwrap_or_else(IdempotencyKey::generate),
    })))
}

fn parse_correct(args: Vec<String>, mut global: GlobalOptions) -> Result<CliAction, DennettError> {
    let mut it = args.into_iter();
    let mut note_id: Option<MemoryNoteId> = None;
    let mut scope: Option<MemoryScope> = None;
    let mut expected_revision: Option<i64> = None;
    let mut title: Option<String> = None;
    let mut body: Option<TextSource> = None;
    let mut rationale: Option<String> = None;
    let mut tags: Vec<String> = Vec::new();
    let mut evidence_links: Vec<EvidenceLinkInput> = Vec::new();
    let mut agent_label: Option<String> = None;
    let mut idempotency_key: Option<IdempotencyKey> = None;

    while let Some(arg) = it.next() {
        if consume_global_flag(&arg, &mut it, &mut global)? {
            continue;
        }
        match arg.as_str() {
            "--id" => note_id = Some(MemoryNoteId::from_str(&next_val(&mut it, "--id")?)?),
            "--scope" => scope = Some(MemoryScope::try_new(next_val(&mut it, "--scope")?)?),
            "--expected-revision" => {
                let raw = next_val(&mut it, "--expected-revision")?;
                let parsed = raw.parse::<i64>().map_err(|_| DennettError::InvalidInput {
                    field: String::from("--expected-revision"),
                    reason: String::from("must be a positive integer"),
                })?;
                expected_revision = Some(parsed);
            }
            "--title" => title = Some(next_val(&mut it, "--title")?),
            "--body" => body = Some(TextSource::Inline(next_val(&mut it, "--body")?)),
            "--stdin" => body = Some(TextSource::Stdin),
            "--rationale" => rationale = Some(next_val(&mut it, "--rationale")?),
            "--tag" => tags.push(next_val(&mut it, "--tag")?),
            "--evidence-id" => {
                let id = EvidenceId::from_str(&next_val(&mut it, "--evidence-id")?)?;
                evidence_links.push(EvidenceLinkInput {
                    evidence_id: id,
                    relationship: EvidenceRelationship::try_new("correction_basis")?,
                    anchor: None,
                    ordinal: None,
                });
            }
            "--evidence-link-json" => {
                let raw = next_val(&mut it, "--evidence-link-json")?;
                let parsed: EvidenceLinkInput =
                    serde_json::from_str(&raw).map_err(|err| DennettError::InvalidInput {
                        field: String::from("--evidence-link-json"),
                        reason: format!("invalid JSON link: {err}"),
                    })?;
                evidence_links.push(parsed);
            }
            "--agent" => agent_label = Some(next_val(&mut it, "--agent")?),
            "--idempotency-key" => {
                idempotency_key = Some(IdempotencyKey::try_new(next_val(
                    &mut it,
                    "--idempotency-key",
                )?)?);
            }
            other => return Err(reject_unknown_flag(other)),
        }
    }

    Ok(CliAction::Correct(Box::new(CorrectArgs {
        global,
        note_id: require_field(note_id, "--id")?,
        scope: require_field(scope, "--scope")?,
        expected_revision: require_field(expected_revision, "--expected-revision")?,
        title: require_field(title, "--title")?,
        body: require_field(body, "--body or --stdin")?,
        rationale: require_field(rationale, "--rationale")?,
        tags,
        evidence_links,
        agent_label,
        idempotency_key: idempotency_key.unwrap_or_else(IdempotencyKey::generate),
    })))
}

fn parse_search(args: Vec<String>, mut global: GlobalOptions) -> Result<CliAction, DennettError> {
    let mut it = args.into_iter();
    let mut scope: Option<MemoryScope> = None;
    let mut query: Option<String> = None;
    let mut limit: Option<u32> = None;
    let mut include_history = false;

    while let Some(arg) = it.next() {
        if consume_global_flag(&arg, &mut it, &mut global)? {
            continue;
        }
        match arg.as_str() {
            "--scope" => scope = Some(MemoryScope::try_new(next_val(&mut it, "--scope")?)?),
            "--query" => query = Some(next_val(&mut it, "--query")?),
            "--limit" => {
                let raw = next_val(&mut it, "--limit")?;
                let parsed = raw.parse::<u32>().map_err(|_| DennettError::InvalidInput {
                    field: String::from("--limit"),
                    reason: String::from("must be a positive integer"),
                })?;
                limit = Some(parsed);
            }
            "--include-history" => include_history = true,
            other => return Err(reject_unknown_flag(other)),
        }
    }

    Ok(CliAction::Search {
        global,
        scope,
        query,
        limit,
        include_history,
    })
}

fn parse_single_id<T: FromStr<Err = DennettError>>(
    args: Vec<String>,
    global: &mut GlobalOptions,
    field: &str,
) -> Result<T, DennettError> {
    let mut it = args.into_iter();
    let mut parsed: Option<T> = None;

    while let Some(arg) = it.next() {
        if consume_global_flag(&arg, &mut it, global)? {
            continue;
        }
        match arg.as_str() {
            "--id" => {
                let raw = next_val(&mut it, "--id")?;
                parsed = Some(T::from_str(&raw)?);
            }
            pos if !pos.starts_with('-') && parsed.is_none() => {
                parsed = Some(T::from_str(pos)?);
            }
            other => return Err(reject_unknown_flag(other)),
        }
    }

    require_field(parsed, &format!("--id ({field})"))
}

fn consume_global_flag(
    arg: &str,
    iter: &mut impl Iterator<Item = String>,
    global: &mut GlobalOptions,
) -> Result<bool, DennettError> {
    match arg {
        "--json" => {
            global.json = true;
            Ok(true)
        }
        "--url" => {
            global.url = next_val(iter, "--url")?;
            Ok(true)
        }
        "--correlation-id" => {
            let raw = next_val(iter, "--correlation-id")?;
            global.correlation_id = Some(CorrelationId::try_new(raw)?);
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn next_val(iter: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, DennettError> {
    iter.next().ok_or_else(|| DennettError::InvalidInput {
        field: flag.to_owned(),
        reason: format!("expected value after `{flag}`"),
    })
}

fn require_field<T>(value: Option<T>, field: &str) -> Result<T, DennettError> {
    value.ok_or_else(|| DennettError::InvalidInput {
        field: field.to_owned(),
        reason: format!("missing required option `{field}`"),
    })
}

fn reject_unknown_flag(arg: &str) -> DennettError {
    if arg == "--token" || arg.starts_with("--token=") {
        return DennettError::InvalidInput {
            field: String::from("--token"),
            reason: format!(
                "bearer tokens are forbidden on the command line; pass via `{ENV_MEMORY_TOKEN}` env var"
            ),
        };
    }
    let safe = sanitize_cli_arg(arg);
    DennettError::InvalidInput {
        field: String::from("cli"),
        reason: format!("unrecognized argument `{safe}`"),
    }
}

fn print_json<T: Serialize>(value: &T) -> Result<(), DennettError> {
    let encoded = serde_json::to_string(value).map_err(|err| DennettError::Internal {
        detail: format!("failed to serialize output JSON: {err}"),
    })?;
    println!("{encoded}");
    Ok(())
}

fn exit_code_for_error(err: &DennettError) -> ExitCode {
    match err {
        DennettError::InvalidInput { .. } => ExitCode::from(2),
        DennettError::Unauthorized { .. } | DennettError::AuthRequired { .. } => ExitCode::from(3),
        DennettError::ForbiddenScope { .. }
        | DennettError::AccessDenied { .. }
        | DennettError::SafetyGuardRejected { .. } => ExitCode::from(4),
        DennettError::NotFound { .. } => ExitCode::from(5),
        DennettError::Conflict { .. } | DennettError::IdempotencyMismatch { .. } => {
            ExitCode::from(6)
        }
        DennettError::DatabaseUnavailable { .. }
        | DennettError::Timeout { .. }
        | DennettError::Cancelled { .. } => ExitCode::from(7),
        _ => ExitCode::from(1),
    }
}

fn emit_error(err: &DennettError, json_mode: bool, token: Option<&str>) {
    let raw = if json_mode {
        serde_json::to_string(err)
            .unwrap_or_else(|_| format!("{{\"kind\":\"internal\",\"code\":\"{}\"}}", err.code()))
    } else {
        format!("[{}] {err}", err.code())
    };
    let safe = match token {
        Some(t) => redact_secrets(&raw, &[t]),
        None => redact_secrets(&raw, &[]),
    };
    eprintln!("{safe}");
}

fn print_usage() {
    println!(
        "dennett-memory-cli — thin CLI facade for standalone dennett-memoryd\n\
         \n\
         Usage:\n\
         \x20 dennett-memory-cli [--url <URL>] [--json] [--correlation-id <ID>] <COMMAND> [OPTIONS]\n\
         \n\
         Commands:\n\
         \x20 health\n\
         \x20 ingest-evidence --scope <SCOPE> [--media-type <MIME>] (--content <TEXT> | --stdin) [--source-uri <URI>] [--idempotency-key <KEY>]\n\
         \x20 write           --scope <SCOPE> --title <TITLE> (--body <TEXT> | --stdin) [--tag <TAG>...] [--admission-status <S>] [--salience <0..100>] [--topic-key <K>] [--evidence-id <UUID>...] [--evidence-link-json <JSON>...] [--agent <LABEL>] [--idempotency-key <KEY>]\n\
         \x20 get             --id <NOTE_UUID> | <NOTE_UUID>\n\
         \x20 history         --id <NOTE_UUID> | <NOTE_UUID>\n\
         \x20 correct         --id <NOTE_UUID> --scope <SCOPE> --expected-revision <N> --title <TITLE> (--body <TEXT> | --stdin) --rationale <TEXT> [--tag <TAG>...] [--evidence-id <UUID>...] [--evidence-link-json <JSON>...] [--agent <LABEL>] [--idempotency-key <KEY>]\n\
         \x20 search          [--scope <SCOPE>] [--query <TEXT>] [--limit <N>] [--include-history]\n\
         \x20 event           --id <EVENT_UUID> | <EVENT_UUID>\n\
         \x20 claim create    --scope <SCOPE> --topic-key <TOPIC> [--claim-key <KEY>] --text <TEXT> [--status <S>] [--caveat <TEXT>...] [--valid-from <MS>] [--valid-to <MS>] [--source-note <UUID>] [--idempotency-key <KEY>]\n\
         \x20 claim get       <CLAIM_UUID>\n\
         \x20 claim supersede <CLAIM_UUID> [--scope <SCOPE>] [--topic-key <TOPIC>] --text <TEXT> [--caveat <TEXT>...] [--valid-from <MS>] [--resolution-event <UUID>] [--idempotency-key <KEY>]\n\
         \x20 claim list      [--scope <SCOPE>] [--topic-key <TOPIC>]\n\
         \x20 conflict list   [--scope <SCOPE>]\n\
         \x20 conflict resolve <CONFLICT_UUID> [--scope <SCOPE>] [--winning-claim <CLAIM_UUID>] [--status <S>] --note <TEXT> [--idempotency-key <KEY>]\n\
         \x20 document list   [--scope <SCOPE>]\n\
         \x20 document create --scope <SCOPE> --key <KEY> --title <TITLE> (--body <TEXT> | --stdin) [--claim-id <UUID>...] [--note-id <UUID>...] [--idempotency-key <KEY>]\n\
         \x20 document get    <DOC_UUID>\n\
         \x20 document update <DOC_UUID> [--scope <SCOPE>] --title <TITLE> (--body <TEXT> | --stdin) --expected-version <N> [--claim-id <UUID>...] [--note-id <UUID>...] [--diff-summary <TEXT>] [--idempotency-key <KEY>]\n\
         \x20 document versions <DOC_UUID>\n\
         \x20 forget          --scope <SCOPE> --target-type <note|evidence|claim|document> --target-id <UUID> --reason <TEXT> [--idempotency-key <KEY>]\n\
         \x20 forget tombstones [--scope <SCOPE>]\n\
         \x20 export          --scope <SCOPE> [--format-version <V>] [--output <FILE>]\n\
         \x20 import          --scope <SCOPE> (--input <FILE> | --stdin)\n\
         \x20 index status    [--scope <SCOPE>]\n\
         \x20 index rebuild   --scope <SCOPE>\n\
         \x20 stats           [--scope <SCOPE>]\n\
         \x20 consolidation list   [--scope <SCOPE>]\n\
         \x20 consolidation create --scope <SCOPE> --type <episode_summary|dossier_update|claim_extraction|index_rebuild|conflict_scan|retention_sweep> [--idempotency-key <KEY>]\n\
         \x20 consolidation pause  <JOB_UUID>\n\
         \x20 consolidation resume <JOB_UUID>\n\
         \n\
         Environment:\n\
         \x20 DENNETT_MEMORY_URL   Base URL of dennett-memoryd (default: {DEFAULT_MEMORY_URL})\n\
         \x20 DENNETT_MEMORY_TOKEN Scoped bearer token (required for /v1/memory/*; NEVER pass via CLI flags)\n\
         \n\
         Exit Codes:\n\
         \x20 0=ok  2=invalid_input  3=unauthorized  4=forbidden_scope  5=not_found  6=conflict/idempotency  7=unavailable/timeout  1=internal"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_rejects_token_flag_without_leaking_secret() {
        let secret = "CLI_LEAK_TOKEN_777888";
        let res = parse_cli(
            ["search".to_owned(), format!("--token={secret}")],
            DEFAULT_MEMORY_URL,
        );
        let Err(err) = res else {
            panic!("--token flag must be rejected");
        };
        let msg = err.to_string();
        assert!(!msg.contains(secret), "token leaked into error: {msg}");
        assert_eq!(exit_code_for_error(&err), ExitCode::from(2));
    }

    #[test]
    fn cli_parses_write_and_keeps_agent_as_untrusted_label() {
        let parsed = parse_cli(
            [
                "--json",
                "write",
                "--scope",
                "project-alpha",
                "--title",
                "Architecture note",
                "--body",
                "Standalone memoryd",
                "--agent",
                "spoofed-root-admin",
                "--idempotency-key",
                "op-cli-1",
            ]
            .into_iter()
            .map(String::from),
            DEFAULT_MEMORY_URL,
        )
        .expect("valid write args");

        let CliAction::Write(write) = parsed else {
            panic!("expected Write action");
        };
        assert!(write.global.json);
        assert_eq!(write.scope.as_str(), "project-alpha");
        assert_eq!(write.agent_label.as_deref(), Some("spoofed-root-admin"));
        assert_eq!(write.idempotency_key.as_str(), "op-cli-1");
    }

    #[test]
    #[expect(
        clippy::too_many_lines,
        reason = "single unit test validates flag parsing across all D3 CLI subcommands"
    )]
    fn cli_parses_all_d3_subcommands() {
        let claim_id = ClaimId::new();
        let conflict_id = ConflictId::new();
        let doc_id = DocumentId::new();
        let job_id = ConsolidationJobId::new();

        assert!(matches!(
            parse_cli(
                [
                    "claim",
                    "create",
                    "--scope",
                    "alpha",
                    "--topic-key",
                    "db",
                    "--text",
                    "Use PG16",
                    "--caveat",
                    "Except offline client cache",
                ]
                .into_iter()
                .map(String::from),
                DEFAULT_MEMORY_URL,
            ),
            Ok(CliAction::ClaimCreate(_))
        ));
        assert!(matches!(
            parse_cli(
                [
                    "claim",
                    "supersede",
                    &claim_id.to_string(),
                    "--text",
                    "Use PG17"
                ]
                .into_iter()
                .map(String::from),
                DEFAULT_MEMORY_URL,
            ),
            Ok(CliAction::ClaimSupersede(_))
        ));
        assert!(matches!(
            parse_cli(
                [
                    "conflict",
                    "resolve",
                    &conflict_id.to_string(),
                    "--winning-claim",
                    &claim_id.to_string(),
                    "--note",
                    "Owner confirmed",
                ]
                .into_iter()
                .map(String::from),
                DEFAULT_MEMORY_URL,
            ),
            Ok(CliAction::ConflictResolve(_))
        ));
        assert!(matches!(
            parse_cli(
                [
                    "document",
                    "update",
                    &doc_id.to_string(),
                    "--title",
                    "Dossier",
                    "--body",
                    "Updated",
                    "--expected-version",
                    "1",
                ]
                .into_iter()
                .map(String::from),
                DEFAULT_MEMORY_URL,
            ),
            Ok(CliAction::DocumentUpdate(_))
        ));
        assert!(matches!(
            parse_cli(
                [
                    "forget",
                    "--scope",
                    "alpha",
                    "--target-type",
                    "note",
                    "--target-id",
                    &Uuid::new_v4().to_string(),
                    "--reason",
                    "user request",
                ]
                .into_iter()
                .map(String::from),
                DEFAULT_MEMORY_URL,
            ),
            Ok(CliAction::Forget(_))
        ));
        assert!(matches!(
            parse_cli(
                ["forget", "tombstones", "--scope", "alpha"]
                    .into_iter()
                    .map(String::from),
                DEFAULT_MEMORY_URL,
            ),
            Ok(CliAction::ForgetTombstones { .. })
        ));
        assert!(matches!(
            parse_cli(
                ["index", "rebuild", "--scope", "alpha"]
                    .into_iter()
                    .map(String::from),
                DEFAULT_MEMORY_URL,
            ),
            Ok(CliAction::IndexRebuild { .. })
        ));
        assert!(matches!(
            parse_cli(
                [
                    "consolidation",
                    "create",
                    "--scope",
                    "alpha",
                    "--type",
                    "dossier_update"
                ]
                .into_iter()
                .map(String::from),
                DEFAULT_MEMORY_URL,
            ),
            Ok(CliAction::ConsolidationCreate { .. })
        ));
        assert!(matches!(
            parse_cli(
                ["consolidation", "pause", &job_id.to_string()]
                    .into_iter()
                    .map(String::from),
                DEFAULT_MEMORY_URL,
            ),
            Ok(CliAction::ConsolidationPause { .. })
        ));
        assert!(matches!(
            parse_cli(
                ["consolidation", "resume", &job_id.to_string()]
                    .into_iter()
                    .map(String::from),
                DEFAULT_MEMORY_URL,
            ),
            Ok(CliAction::ConsolidationResume { .. })
        ));
    }

    #[test]
    fn exit_codes_are_distinct_and_stable() {
        assert_eq!(
            exit_code_for_error(&DennettError::InvalidInput {
                field: String::from("x"),
                reason: String::from("bad"),
            }),
            ExitCode::from(2)
        );
        assert_eq!(
            exit_code_for_error(&DennettError::Unauthorized {
                reason: String::from("expired"),
            }),
            ExitCode::from(3)
        );
        assert_eq!(
            exit_code_for_error(&DennettError::ForbiddenScope {
                scope: String::from("beta"),
                reason: String::from("denied"),
            }),
            ExitCode::from(4)
        );
        assert_eq!(
            exit_code_for_error(&DennettError::NotFound {
                resource: String::from("note"),
                id: String::from("1"),
            }),
            ExitCode::from(5)
        );
        assert_eq!(
            exit_code_for_error(&DennettError::Conflict {
                resource: String::from("note"),
                expected_revision: Some(1),
                actual_revision: Some(2),
                reason: String::from("stale"),
            }),
            ExitCode::from(6)
        );
        assert_eq!(
            exit_code_for_error(&DennettError::DatabaseUnavailable {
                detail: String::from("down"),
            }),
            ExitCode::from(7)
        );
    }
}
