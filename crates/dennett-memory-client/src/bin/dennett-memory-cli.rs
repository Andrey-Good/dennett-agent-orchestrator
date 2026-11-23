//! Thin CLI facade (`dennett-memory-cli`) backed by [`MemoryClient`] -> `dennett-memoryd`.
//!
//! Architectural invariants (Architecture 82 §7, V06):
//! - Uses the exact same domain implementation as API and MCP by calling `dennett-memoryd` over HTTP.
//! - Reads bearer credentials STRICTLY from `DENNETT_MEMORY_TOKEN` (never accepts a `--token` flag).
//! - Treats `--agent` solely as an untrusted display label (`author_label`), never as principal proof.
//! - Supports `--help`, `--json`, bounded `--stdin` reading, stable exit codes, and secret redaction.

use std::collections::BTreeMap;
use std::env;
use std::io::{self, Read};
use std::process::ExitCode;
use std::str::FromStr;

use dennett_contracts::redact::{redact_secrets, sanitize_cli_arg};
use dennett_contracts::{
    CorrectNoteRequest, CorrelationId, CreateNoteRequest, DennettError, EvidenceId,
    EvidenceLinkInput, EvidenceRelationship, IdempotencyKey, IngestEvidenceRequest,
    MAX_EVIDENCE_CONTENT_BYTES, MemoryEventId, MemoryNoteId, MemoryScope, SearchNotesRequest,
};
use dennett_memory_client::MemoryClient;
use serde::Serialize;

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
    },
    Event {
        global: GlobalOptions,
        event_id: MemoryEventId,
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
        | CliAction::Event { global, .. } => global.json,
        CliAction::Write(w) => w.global.json,
        CliAction::Correct(c) => c.global.json,
        CliAction::Help | CliAction::Version => false,
    }
}

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
                idempotency_key,
            } = *args;
            let req = CreateNoteRequest {
                scope,
                title,
                body: resolve_text(body)?,
                tags,
                observed_at: None,
                evidence_links,
                author_label: agent_label,
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
                body: resolve_text(body)?,
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
        } => {
            exec_search(
                &global,
                token,
                SearchNotesRequest {
                    scope,
                    query,
                    limit,
                },
            )
            .await
        }
        CliAction::Event { global, event_id } => exec_event(&global, token, event_id).await,
        CliAction::Help | CliAction::Version => Ok(()),
    }
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
        content: resolve_text(content)?,
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
            "matched={} returned={} truncated={}",
            resp.total_matched,
            resp.notes.len(),
            resp.truncated
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

fn resolve_text(source: TextSource) -> Result<String, DennettError> {
    match source {
        TextSource::Inline(s) => Ok(s),
        TextSource::Stdin => {
            let mut buf = Vec::new();
            let mut limited = io::stdin()
                .lock()
                .take((MAX_EVIDENCE_CONTENT_BYTES + 1) as u64);
            limited
                .read_to_end(&mut buf)
                .map_err(|err| DennettError::InvalidInput {
                    field: String::from("stdin"),
                    reason: format!("failed to read stdin: {err}"),
                })?;
            if buf.len() > MAX_EVIDENCE_CONTENT_BYTES {
                return Err(DennettError::InvalidInput {
                    field: String::from("stdin"),
                    reason: format!(
                        "stdin content exceeds maximum limit of {MAX_EVIDENCE_CONTENT_BYTES} bytes"
                    ),
                });
            }
            String::from_utf8(buf).map_err(|_| DennettError::InvalidInput {
                field: String::from("stdin"),
                reason: String::from("stdin must be valid UTF-8"),
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
        other => Err(DennettError::InvalidInput {
            field: String::from("subcommand"),
            reason: format!("unknown subcommand `{}`", sanitize_cli_arg(other)),
        }),
    }
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
            other => return Err(reject_unknown_flag(other)),
        }
    }

    Ok(CliAction::Search {
        global,
        scope,
        query,
        limit,
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
         \x20 write           --scope <SCOPE> --title <TITLE> (--body <TEXT> | --stdin) [--tag <TAG>...] [--evidence-id <UUID>...] [--evidence-link-json <JSON>...] [--agent <LABEL>] [--idempotency-key <KEY>]\n\
         \x20 get             --id <NOTE_UUID>\n\
         \x20 history         --id <NOTE_UUID>\n\
         \x20 correct         --id <NOTE_UUID> --scope <SCOPE> --expected-revision <N> --title <TITLE> (--body <TEXT> | --stdin) --rationale <TEXT> [--tag <TAG>...] [--evidence-id <UUID>...] [--evidence-link-json <JSON>...] [--agent <LABEL>] [--idempotency-key <KEY>]\n\
         \x20 search          [--scope <SCOPE>] [--query <TEXT>] [--limit <N>]\n\
         \x20 event           --id <EVENT_UUID>\n\
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
