//! V02-safe Owner Memory Fabric HTTP proxy inside `dennett-head` (`/v1/memory/*`).
//!
//! Architectural boundaries (Architecture 80 §3.1, V02, D3):
//! - Communicates strictly over HTTP via [`MemoryClient`] — NEVER links `dennett-memory-core`.
//! - Mints least-privilege `dnt1.` tokens per owner operation (`Read`, `Write`, `Forget`,
//!   `Export`, `Admin`) while keeping agent `dennett-memory-mcp` tokens restricted to `[Read, Write]`.
//! - `get_memory_overview` NEVER fails with 503 on `memoryd` outage: returns `200 OK` with
//!   `MemoryStatus::DegradedUnreachable` and empty collections without fabricating answers (V02).
//! - Mutating proxy endpoints return `503 DatabaseUnavailable` when `memoryd` is unreachable and
//!   NEVER block `Stop` (`cancel_run`) or core control-plane reads.

use std::collections::BTreeSet;

use dennett_contracts::{
    ConsolidationJob, ConsolidationJobId, CorrectNoteRequest, CorrelationId, CreateClaimRequest,
    CreateConsolidationJobRequest, CreateDocumentRequest, DennettError, DocumentId, ExportRequest,
    ExportResponse, ForgetRequest, ForgetResponse, GetDocumentVersionsResponse, ImportRequest,
    ImportResponse, ListClaimsResponse, ListConflictsResponse, ListConsolidationJobsResponse,
    ListDocumentsResponse, ListTombstonesResponse, MemoryCapability, MemoryClaim, MemoryConflict,
    MemoryDocument, MemoryNoteId, MemoryOverviewResponse, MemoryScope, MemoryStatus,
    NoteHistoryResponse, NoteMutationResponse, ResolveConflictRequest, SearchNotesRequest,
    SupersedeClaimRequest, TriggerIndexRebuildRequest, TriggerIndexRebuildResponse,
    UpdateDocumentRequest,
};

use crate::service::HeadService;

fn degraded_overview(scope: Option<MemoryScope>, reason: String) -> MemoryOverviewResponse {
    MemoryOverviewResponse {
        memory_status: MemoryStatus::DegradedUnreachable,
        degraded_reason: Some(reason),
        scope,
        stats: None,
        notes: Vec::new(),
        claims: Vec::new(),
        conflicts: Vec::new(),
        documents: Vec::new(),
        consolidation_jobs: Vec::new(),
        tombstones: Vec::new(),
    }
}

impl HeadService {
    async fn resolve_proxy_scopes(
        &self,
        explicit_scope: Option<&MemoryScope>,
    ) -> Result<BTreeSet<MemoryScope>, DennettError> {
        let mut scopes = self.collect_all_known_memory_scopes().await?;
        if let Some(sc) = explicit_scope {
            scopes.insert(sc.clone());
        }
        Ok(scopes)
    }

    async fn owner_memory_client(
        &self,
        explicit_scope: Option<&MemoryScope>,
        caps: &[MemoryCapability],
    ) -> Result<dennett_memory_client::MemoryClient, DennettError> {
        let scopes = self.resolve_proxy_scopes(explicit_scope).await?;
        let cap_set: BTreeSet<MemoryCapability> = caps.iter().copied().collect();
        self.memory_client_with_capabilities(scopes, cap_set)?
            .ok_or_else(|| DennettError::DatabaseUnavailable {
                detail: String::from("dennett-memoryd client is not configured on dennett-head"),
            })
    }

    /// V02-safe Memory Fabric overview snapshot (`GET /v1/memory/overview`).
    ///
    /// When `dennett-memoryd` is unreachable or unconfigured, returns `Ok` carrying
    /// `MemoryStatus::DegradedUnreachable`, redacted `degraded_reason`, and empty collections
    /// without fabricating memory items.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError`] only on Head control-plane PostgreSQL failure.
    pub async fn get_memory_overview(
        &self,
        scope: Option<MemoryScope>,
        correlation_id: &CorrelationId,
    ) -> Result<MemoryOverviewResponse, DennettError> {
        let client = match self
            .owner_memory_client(scope.as_ref(), &[MemoryCapability::Read])
            .await
        {
            Ok(c) => c,
            Err(DennettError::DatabaseUnavailable { detail }) => {
                self.record_memory_status_pub(MemoryStatus::DegradedUnreachable);
                return Ok(degraded_overview(scope, self.redact(&detail)));
            }
            Err(err) => return Err(err),
        };

        // First probe stats to detect reachability cleanly under V02.
        let stats = match client.get_stats(scope.as_ref(), Some(correlation_id)).await {
            Ok(s) => {
                self.record_memory_status_pub(MemoryStatus::Healthy);
                s
            }
            Err(
                err @ (DennettError::DatabaseUnavailable { .. } | DennettError::Timeout { .. }),
            ) => {
                self.record_memory_status_pub(MemoryStatus::DegradedUnreachable);
                return Ok(degraded_overview(scope, self.redact(&err.to_string())));
            }
            Err(err) => return Err(err),
        };

        let note_search = SearchNotesRequest {
            scope: scope.clone(),
            query: None,
            limit: Some(50),
            include_history: false,
        };

        let (notes_res, claims_res, conflicts_res, docs_res, jobs_res, tombstones_res) = tokio::join!(
            client.search_notes(&note_search, Some(correlation_id)),
            client.list_claims(scope.as_ref(), None, Some(correlation_id)),
            client.list_open_conflicts(scope.as_ref(), Some(correlation_id)),
            client.list_documents(scope.as_ref(), Some(correlation_id)),
            client.list_consolidation_jobs(scope.as_ref(), Some(correlation_id)),
            client.list_tombstones(scope.as_ref(), Some(correlation_id)),
        );

        // If `memoryd` dropped mid-overview, degrade cleanly rather than returning partial lies.
        for err_opt in [
            notes_res.as_ref().err(),
            claims_res.as_ref().err(),
            conflicts_res.as_ref().err(),
            docs_res.as_ref().err(),
            jobs_res.as_ref().err(),
            tombstones_res.as_ref().err(),
        ]
        .into_iter()
        .flatten()
        {
            if matches!(
                err_opt,
                DennettError::DatabaseUnavailable { .. } | DennettError::Timeout { .. }
            ) {
                self.record_memory_status_pub(MemoryStatus::DegradedUnreachable);
                return Ok(degraded_overview(scope, self.redact(&err_opt.to_string())));
            }
        }

        Ok(MemoryOverviewResponse {
            memory_status: MemoryStatus::Healthy,
            degraded_reason: None,
            scope,
            stats: Some(stats),
            notes: notes_res.map_or_default(|r| r.notes),
            claims: claims_res.map_or_default(|r| r.claims),
            conflicts: conflicts_res.map_or_default(|r| r.conflicts),
            documents: docs_res.map_or_default(|r| r.documents),
            consolidation_jobs: jobs_res.map_or_default(|r| r.jobs),
            tombstones: tombstones_res.map_or_default(|r| r.tombstones),
        })
    }

    /// Proxies `POST /v1/memory/notes/{note_id}/correct` (`Read` + `Write`).
    ///
    /// # Errors
    ///
    /// Propagates [`DennettError`] from `memoryd` or `503 DatabaseUnavailable` when unreachable.
    pub async fn proxy_correct_memory_note(
        &self,
        req: &CorrectNoteRequest,
        correlation_id: &CorrelationId,
    ) -> Result<NoteMutationResponse, DennettError> {
        let client = self
            .owner_memory_client(None, &[MemoryCapability::Read, MemoryCapability::Write])
            .await?;
        let res = client.correct_note(req, Some(correlation_id)).await;
        self.observe_memory_proxy_result(&res);
        res
    }

    /// Proxies `GET /v1/memory/notes/{note_id}/history` (`Read`).
    ///
    /// # Errors
    ///
    /// Propagates [`DennettError`] from `memoryd` or `503 DatabaseUnavailable` when unreachable.
    pub async fn proxy_get_memory_note_history(
        &self,
        note_id: MemoryNoteId,
        correlation_id: &CorrelationId,
    ) -> Result<NoteHistoryResponse, DennettError> {
        let client = self
            .owner_memory_client(None, &[MemoryCapability::Read])
            .await?;
        let res = client.get_note_history(note_id, Some(correlation_id)).await;
        self.observe_memory_proxy_result(&res);
        res
    }

    /// Proxies `GET /v1/memory/claims` (`Read`).
    ///
    /// # Errors
    ///
    /// Propagates [`DennettError`] from `memoryd` or `503 DatabaseUnavailable` when unreachable.
    pub async fn proxy_list_memory_claims(
        &self,
        scope: Option<&MemoryScope>,
        topic_key: Option<&str>,
        correlation_id: &CorrelationId,
    ) -> Result<ListClaimsResponse, DennettError> {
        let client = self
            .owner_memory_client(scope, &[MemoryCapability::Read])
            .await?;
        let res = client
            .list_claims(scope, topic_key, Some(correlation_id))
            .await;
        self.observe_memory_proxy_result(&res);
        res
    }

    /// Proxies `POST /v1/memory/claims` (`Read` + `Write`).
    ///
    /// # Errors
    ///
    /// Propagates [`DennettError`] from `memoryd` or `503 DatabaseUnavailable` when unreachable.
    pub async fn proxy_create_memory_claim(
        &self,
        req: &CreateClaimRequest,
        correlation_id: &CorrelationId,
    ) -> Result<MemoryClaim, DennettError> {
        let client = self
            .owner_memory_client(
                Some(&req.scope),
                &[MemoryCapability::Read, MemoryCapability::Write],
            )
            .await?;
        let res = client.create_claim(req, Some(correlation_id)).await;
        self.observe_memory_proxy_result(&res);
        res
    }

    /// Proxies `POST /v1/memory/claims/{claim_id}/supersede` (`Read` + `Write`).
    ///
    /// # Errors
    ///
    /// Propagates [`DennettError`] from `memoryd` or `503 DatabaseUnavailable` when unreachable.
    pub async fn proxy_supersede_memory_claim(
        &self,
        req: &SupersedeClaimRequest,
        correlation_id: &CorrelationId,
    ) -> Result<MemoryClaim, DennettError> {
        let client = self
            .owner_memory_client(None, &[MemoryCapability::Read, MemoryCapability::Write])
            .await?;
        let res = client.supersede_claim(req, Some(correlation_id)).await;
        self.observe_memory_proxy_result(&res);
        res
    }

    /// Proxies `GET /v1/memory/conflicts` (`Read`).
    ///
    /// # Errors
    ///
    /// Propagates [`DennettError`] from `memoryd` or `503 DatabaseUnavailable` when unreachable.
    pub async fn proxy_list_memory_conflicts(
        &self,
        scope: Option<&MemoryScope>,
        correlation_id: &CorrelationId,
    ) -> Result<ListConflictsResponse, DennettError> {
        let client = self
            .owner_memory_client(scope, &[MemoryCapability::Read])
            .await?;
        let res = client
            .list_open_conflicts(scope, Some(correlation_id))
            .await;
        self.observe_memory_proxy_result(&res);
        res
    }

    /// Proxies `POST /v1/memory/conflicts/{conflict_id}/resolve` (`Read` + `Write`).
    ///
    /// # Errors
    ///
    /// Propagates [`DennettError`] from `memoryd` or `503 DatabaseUnavailable` when unreachable.
    pub async fn proxy_resolve_memory_conflict(
        &self,
        req: &ResolveConflictRequest,
        correlation_id: &CorrelationId,
    ) -> Result<MemoryConflict, DennettError> {
        let client = self
            .owner_memory_client(None, &[MemoryCapability::Read, MemoryCapability::Write])
            .await?;
        let res = client.resolve_conflict(req, Some(correlation_id)).await;
        self.observe_memory_proxy_result(&res);
        res
    }

    /// Proxies `GET /v1/memory/documents` (`Read`).
    ///
    /// # Errors
    ///
    /// Propagates [`DennettError`] from `memoryd` or `503 DatabaseUnavailable` when unreachable.
    pub async fn proxy_list_memory_documents(
        &self,
        scope: Option<&MemoryScope>,
        correlation_id: &CorrelationId,
    ) -> Result<ListDocumentsResponse, DennettError> {
        let client = self
            .owner_memory_client(scope, &[MemoryCapability::Read])
            .await?;
        let res = client.list_documents(scope, Some(correlation_id)).await;
        self.observe_memory_proxy_result(&res);
        res
    }

    /// Proxies `POST /v1/memory/documents` (`Read` + `Write`).
    ///
    /// # Errors
    ///
    /// Propagates [`DennettError`] from `memoryd` or `503 DatabaseUnavailable` when unreachable.
    pub async fn proxy_create_memory_document(
        &self,
        req: &CreateDocumentRequest,
        correlation_id: &CorrelationId,
    ) -> Result<MemoryDocument, DennettError> {
        let client = self
            .owner_memory_client(
                Some(&req.scope),
                &[MemoryCapability::Read, MemoryCapability::Write],
            )
            .await?;
        let res = client.create_document(req, Some(correlation_id)).await;
        self.observe_memory_proxy_result(&res);
        res
    }

    /// Proxies `GET /v1/memory/documents/{document_id}` (`Read`).
    ///
    /// # Errors
    ///
    /// Propagates [`DennettError`] from `memoryd` or `503 DatabaseUnavailable` when unreachable.
    pub async fn proxy_get_memory_document(
        &self,
        document_id: DocumentId,
        correlation_id: &CorrelationId,
    ) -> Result<MemoryDocument, DennettError> {
        let client = self
            .owner_memory_client(None, &[MemoryCapability::Read])
            .await?;
        let res = client.get_document(document_id, Some(correlation_id)).await;
        self.observe_memory_proxy_result(&res);
        res
    }

    /// Proxies `PUT /v1/memory/documents/{document_id}` (`Read` + `Write`).
    ///
    /// # Errors
    ///
    /// Propagates [`DennettError`] from `memoryd` or `503 DatabaseUnavailable` when unreachable.
    pub async fn proxy_update_memory_document(
        &self,
        req: &UpdateDocumentRequest,
        correlation_id: &CorrelationId,
    ) -> Result<MemoryDocument, DennettError> {
        let client = self
            .owner_memory_client(None, &[MemoryCapability::Read, MemoryCapability::Write])
            .await?;
        let res = client.update_document(req, Some(correlation_id)).await;
        self.observe_memory_proxy_result(&res);
        res
    }

    /// Proxies `GET /v1/memory/documents/{document_id}/versions` (`Read`).
    ///
    /// # Errors
    ///
    /// Propagates [`DennettError`] from `memoryd` or `503 DatabaseUnavailable` when unreachable.
    pub async fn proxy_get_memory_document_versions(
        &self,
        document_id: DocumentId,
        correlation_id: &CorrelationId,
    ) -> Result<GetDocumentVersionsResponse, DennettError> {
        let client = self
            .owner_memory_client(None, &[MemoryCapability::Read])
            .await?;
        let res = client
            .get_document_versions(document_id, Some(correlation_id))
            .await;
        self.observe_memory_proxy_result(&res);
        res
    }

    /// Proxies `POST /v1/memory/forget` (`Forget`).
    ///
    /// # Errors
    ///
    /// Propagates [`DennettError`] from `memoryd` or `503 DatabaseUnavailable` when unreachable.
    pub async fn proxy_forget_memory_item(
        &self,
        req: &ForgetRequest,
        correlation_id: &CorrelationId,
    ) -> Result<ForgetResponse, DennettError> {
        let client = self
            .owner_memory_client(
                Some(&req.scope),
                &[MemoryCapability::Read, MemoryCapability::Forget],
            )
            .await?;
        let res = client.forget(req, Some(correlation_id)).await;
        self.observe_memory_proxy_result(&res);
        res
    }

    /// Proxies `GET /v1/memory/forget/tombstones` (`Read`).
    ///
    /// # Errors
    ///
    /// Propagates [`DennettError`] from `memoryd` or `503 DatabaseUnavailable` when unreachable.
    pub async fn proxy_list_memory_tombstones(
        &self,
        scope: Option<&MemoryScope>,
        correlation_id: &CorrelationId,
    ) -> Result<ListTombstonesResponse, DennettError> {
        let client = self
            .owner_memory_client(scope, &[MemoryCapability::Read])
            .await?;
        let res = client.list_tombstones(scope, Some(correlation_id)).await;
        self.observe_memory_proxy_result(&res);
        res
    }

    /// Proxies `POST /v1/memory/export` (`Read` + `Export`).
    ///
    /// # Errors
    ///
    /// Propagates [`DennettError`] from `memoryd` or `503 DatabaseUnavailable` when unreachable.
    pub async fn proxy_export_memory_package(
        &self,
        req: &ExportRequest,
        correlation_id: &CorrelationId,
    ) -> Result<ExportResponse, DennettError> {
        let client = self
            .owner_memory_client(
                Some(&req.scope),
                &[MemoryCapability::Read, MemoryCapability::Export],
            )
            .await?;
        let res = client.export_package(req, Some(correlation_id)).await;
        self.observe_memory_proxy_result(&res);
        res
    }

    /// Proxies `POST /v1/memory/import` (`Read` + `Write`).
    ///
    /// # Errors
    ///
    /// Propagates [`DennettError`] from `memoryd` or `503 DatabaseUnavailable` when unreachable.
    pub async fn proxy_import_memory_package(
        &self,
        req: &ImportRequest,
        correlation_id: &CorrelationId,
    ) -> Result<ImportResponse, DennettError> {
        let client = self
            .owner_memory_client(
                Some(&req.scope),
                &[MemoryCapability::Read, MemoryCapability::Write],
            )
            .await?;
        let res = client.import_package(req, Some(correlation_id)).await;
        self.observe_memory_proxy_result(&res);
        res
    }

    /// Proxies `POST /v1/memory/index/rebuild` (`Read` + `Admin`).
    ///
    /// # Errors
    ///
    /// Propagates [`DennettError`] from `memoryd` or `503 DatabaseUnavailable` when unreachable.
    pub async fn proxy_trigger_memory_index_rebuild(
        &self,
        req: &TriggerIndexRebuildRequest,
        correlation_id: &CorrelationId,
    ) -> Result<TriggerIndexRebuildResponse, DennettError> {
        let client = self
            .owner_memory_client(
                Some(&req.scope),
                &[MemoryCapability::Read, MemoryCapability::Admin],
            )
            .await?;
        let res = client
            .trigger_index_rebuild(req, Some(correlation_id))
            .await;
        self.observe_memory_proxy_result(&res);
        res
    }

    /// Proxies `GET /v1/memory/consolidation/jobs` (`Read`).
    ///
    /// # Errors
    ///
    /// Propagates [`DennettError`] from `memoryd` or `503 DatabaseUnavailable` when unreachable.
    pub async fn proxy_list_memory_consolidation_jobs(
        &self,
        scope: Option<&MemoryScope>,
        correlation_id: &CorrelationId,
    ) -> Result<ListConsolidationJobsResponse, DennettError> {
        let client = self
            .owner_memory_client(scope, &[MemoryCapability::Read])
            .await?;
        let res = client
            .list_consolidation_jobs(scope, Some(correlation_id))
            .await;
        self.observe_memory_proxy_result(&res);
        res
    }

    /// Proxies `POST /v1/memory/consolidation/jobs` (`Read` + `Write`).
    ///
    /// # Errors
    ///
    /// Propagates [`DennettError`] from `memoryd` or `503 DatabaseUnavailable` when unreachable.
    pub async fn proxy_create_memory_consolidation_job(
        &self,
        req: &CreateConsolidationJobRequest,
        correlation_id: &CorrelationId,
    ) -> Result<ConsolidationJob, DennettError> {
        let client = self
            .owner_memory_client(
                Some(&req.scope),
                &[MemoryCapability::Read, MemoryCapability::Write],
            )
            .await?;
        let res = client
            .create_consolidation_job(req, Some(correlation_id))
            .await;
        self.observe_memory_proxy_result(&res);
        res
    }

    /// Proxies `POST /v1/memory/consolidation/jobs/{job_id}/pause` (`Read` + `Write`).
    ///
    /// # Errors
    ///
    /// Propagates [`DennettError`] from `memoryd` or `503 DatabaseUnavailable` when unreachable.
    pub async fn proxy_pause_memory_consolidation_job(
        &self,
        job_id: ConsolidationJobId,
        correlation_id: &CorrelationId,
    ) -> Result<ConsolidationJob, DennettError> {
        let client = self
            .owner_memory_client(None, &[MemoryCapability::Read, MemoryCapability::Write])
            .await?;
        let res = client
            .pause_consolidation_job(job_id, Some(correlation_id))
            .await;
        self.observe_memory_proxy_result(&res);
        res
    }

    /// Proxies `POST /v1/memory/consolidation/jobs/{job_id}/resume` (`Read` + `Write`).
    ///
    /// # Errors
    ///
    /// Propagates [`DennettError`] from `memoryd` or `503 DatabaseUnavailable` when unreachable.
    pub async fn proxy_resume_memory_consolidation_job(
        &self,
        job_id: ConsolidationJobId,
        correlation_id: &CorrelationId,
    ) -> Result<ConsolidationJob, DennettError> {
        let client = self
            .owner_memory_client(None, &[MemoryCapability::Read, MemoryCapability::Write])
            .await?;
        let res = client
            .resume_consolidation_job(job_id, Some(correlation_id))
            .await;
        self.observe_memory_proxy_result(&res);
        res
    }

    fn observe_memory_proxy_result<T>(&self, res: &Result<T, DennettError>) {
        match res {
            Ok(_) => self.record_memory_status_pub(MemoryStatus::Healthy),
            Err(DennettError::DatabaseUnavailable { .. } | DennettError::Timeout { .. }) => {
                self.record_memory_status_pub(MemoryStatus::DegradedUnreachable);
            }
            Err(_) => {}
        }
    }
}
