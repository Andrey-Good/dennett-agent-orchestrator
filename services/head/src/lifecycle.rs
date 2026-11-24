//! Strict Project & Artifact lifecycle separation (Contracts C & D, Scenario 90 §30).
//!
//! Fundamental safety invariant:
//! - Dennett NEVER deletes external user workspace folders or files on disk during `rebind`,
//!   `detach-workspace`, `archive`, `unarchive`, `remove`, or `delete` (`user_workspace_files_deleted`
//!   is unconditionally `false`!).
//! - `delete` on a project removes ONLY Dennett's internal detached scratch directory (under
//!   `detached_projects_root`) and scrubs inline Dennett metadata/content while leaving external
//!   user directories 100% untouched.

use std::fs;
use std::str::FromStr;

use dennett_contracts::{
    ArtifactCompleteness, ArtifactId, ArtifactLifecycleAction, ArtifactLifecycleRequest,
    ArtifactLifecycleResponse, ArtifactLifecycleState, CorrelationId, DennettError,
    ListArtifactsResponse, PermissionRequestId, ProjectId, ProjectLifecycleResponse,
    ProjectLifecycleState, RebindProjectRequest, SessionId, TaskId,
};
use serde_json::json;

use crate::events::{ControlEventAppend, append_control_event_in_tx};
use crate::service::{DEFAULT_PG_OP_TIMEOUT, HeadService};
use crate::store::{
    ARTIFACT_SELECT_COLS, PROJECT_SELECT_COLS, append_audit_event, decode_artifact_row,
    decode_project_row, fetch_project, fetch_session, map_pg_err, truncate_utf8,
};
use crate::workspace_files::{clean_display_path, validate_safe_workspace_root};

struct TerminatedProjectWork {
    affected_timers: usize,
    cancelled_task_ids: Vec<TaskId>,
    session_ids: Vec<SessionId>,
    expired_perms: Vec<(PermissionRequestId, SessionId)>,
}

impl HeadService {
    /// Atomically cancels active/waiting tasks, timers, runs, open inbox cards, pending permission
    /// requests, active capability grants, and active sessions for a removed/deleted project (H2-03).
    #[expect(
        clippy::too_many_lines,
        reason = "cancels timers, tasks, runs, permission requests, inbox cards, capability grants, and sessions atomically in one transaction"
    )]
    async fn terminate_project_work_in_tx<C: deadpool_postgres::GenericClient>(
        &self,
        tx: &C,
        project_id: ProjectId,
        reason: &str,
    ) -> Result<TerminatedProjectWork, DennettError> {
        let cancelled_timers = tx
            .execute(
                "UPDATE task_timers t
                 SET status = 'cancelled'
                 FROM tasks tk
                 WHERE t.task_id = tk.task_id
                   AND tk.project_id = $1
                   AND t.status IN ('pending', 'claimed', 'paused')",
                &[&project_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let affected_timers = usize::try_from(cancelled_timers).unwrap_or(0);

        let task_rows = tx
            .query(
                "UPDATE tasks
                 SET status = 'cancelled',
                     active_run_id = NULL,
                     wait_detail = NULL,
                     expected_signal_name = NULL,
                     finished_at = COALESCE(finished_at, now()),
                     updated_at = now()
                 WHERE project_id = $1
                   AND status NOT IN ('succeeded', 'failed', 'cancelled')
                 RETURNING task_id",
                &[&project_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let cancelled_task_ids: Vec<TaskId> = task_rows
            .iter()
            .map(|r| TaskId::from_uuid(r.get(0)))
            .collect();

        tx.execute(
            "UPDATE runs r
             SET status = 'cancelled',
                 stop_reason = 'project_removed_or_deleted',
                 finished_at = now()
             FROM sessions s
             WHERE r.session_id = s.session_id
               AND s.project_id = $1
               AND r.status = 'running'",
            &[&project_id.as_uuid()],
        )
        .await
        .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let perm_rows = tx
            .query(
                "UPDATE permission_requests
                 SET status = 'expired',
                     revision = revision + 1,
                     resolved_at = now()
                 WHERE project_id = $1 AND status = 'pending'
                 RETURNING request_id, session_id",
                &[&project_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let expired_perms: Vec<(PermissionRequestId, SessionId)> = perm_rows
            .iter()
            .map(|r| {
                (
                    PermissionRequestId::from_uuid(r.get(0)),
                    SessionId::from_uuid(r.get(1)),
                )
            })
            .collect();

        tx.execute(
            "UPDATE inbox_cards
             SET status = 'withdrawn',
                 resolution_summary = $2,
                 revision = revision + 1,
                 resolved_at = now(),
                 updated_at = now()
             WHERE project_id = $1 AND status = 'open'",
            &[&project_id.as_uuid(), &reason],
        )
        .await
        .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        tx.execute(
            "UPDATE capability_grants
             SET revoked_at = COALESCE(revoked_at, now()),
                 revoked_reason = COALESCE(revoked_reason, $2)
             WHERE project_id = $1 AND revoked_at IS NULL",
            &[&project_id.as_uuid(), &reason],
        )
        .await
        .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let session_rows = tx
            .query(
                "UPDATE sessions
                 SET status = 'closed',
                     updated_at = now()
                 WHERE project_id = $1
                 RETURNING session_id",
                &[&project_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let session_ids: Vec<SessionId> = session_rows
            .iter()
            .map(|r| SessionId::from_uuid(r.get(0)))
            .collect();

        Ok(TerminatedProjectWork {
            affected_timers,
            cancelled_task_ids,
            session_ids,
            expired_perms,
        })
    }

    async fn finish_project_work_shutdown(&self, work: TerminatedProjectWork, reason: &str) {
        for tid in work.cancelled_task_ids {
            let _ = self.signal_task_cancel_only(tid).await;
        }
        for (perm_id, sid) in work.expired_perms {
            self.wake_expired_approval_waiters(&[perm_id], sid, reason)
                .await;
        }
        for sid in work.session_ids {
            let _ = self.cancel_active_session_run(sid).await;
            let _ = self.runtime().close_session(sid).await;
            if let Some(bg) = self.background_runtime() {
                let _ = bg.close_session(sid).await;
            }
        }
    }

    /// Rebinds a project to a new canonical local workspace folder (`POST /v1/projects/{id}/rebind`,
    /// Contract C), recording old->new path provenance and closing stale live runtime processes
    /// so the next turn cleanly rehydrates on the new `cwd`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`], [`DennettError::AccessDenied`],
    /// [`DennettError::NotFound`], [`DennettError::Conflict`], or [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_lines,
        reason = "validates canonical workspace root, updates binding, records provenance event, and closes stale runtime sessions atomically"
    )]
    pub async fn rebind_project(
        &self,
        project_id: ProjectId,
        req: RebindProjectRequest,
        correlation_id: &CorrelationId,
    ) -> Result<ProjectLifecycleResponse, DennettError> {
        req.validate()?;
        let _ = self.ensure_migrated().await?;

        let raw_path = req.workspace_path.clone();
        let scope_label = format!("project:{project_id}");
        let canonical = tokio::task::spawn_blocking(move || {
            validate_safe_workspace_root(&raw_path, &scope_label)
        })
        .await
        .map_err(|err| DennettError::Internal {
            detail: format!("rebind workspace validation task failed: {err}"),
        })??;
        let next_path = clean_display_path(&canonical);

        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let existing = fetch_project(&tx, project_id, self.pg_target()).await?;
        if existing.lifecycle_state == ProjectLifecycleState::DeletedRecord {
            return Err(DennettError::Conflict {
                resource: String::from("project"),
                expected_revision: None,
                actual_revision: None,
                reason: String::from("cannot rebind a deleted project record"),
            });
        }
        let next_lifecycle = if existing.lifecycle_state == ProjectLifecycleState::Archived {
            ProjectLifecycleState::Archived
        } else {
            ProjectLifecycleState::Active
        };

        let sql = format!(
            "UPDATE projects
             SET workspace_path = $2,
                 lifecycle_state = $3,
                 updated_at = now()
             WHERE project_id = $1
             RETURNING {PROJECT_SELECT_COLS}"
        );
        let row = tx
            .query_one(
                &sql,
                &[
                    &project_id.as_uuid(),
                    &Some(next_path.as_str()),
                    &next_lifecycle.as_str(),
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let updated = decode_project_row(&row)?;

        let session_rows = tx
            .query(
                "SELECT session_id FROM sessions WHERE project_id = $1",
                &[&project_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let session_ids: Vec<SessionId> = session_rows
            .iter()
            .map(|r| SessionId::from_uuid(r.get(0)))
            .collect();

        let _ = append_audit_event(
            &tx,
            self.pg_target(),
            correlation_id,
            None,
            None,
            "head",
            "project_rebound",
            &self.redact(&format!(
                "rebound project {project_id} workspace from {:?} to {:?}",
                existing.workspace_path, updated.workspace_path
            )),
        )
        .await?;

        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &format!("project:{project_id}"),
                event_type: "project.rebound",
                project_id: Some(project_id),
                session_id: None,
                task_id: None,
                correlation_id,
                payload: json!({
                    "project_id": project_id,
                    "previous_workspace_path": existing.workspace_path,
                    "workspace_path": updated.workspace_path,
                    "lifecycle_state": updated.lifecycle_state,
                    "reason": req.reason,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        // Close live runtime handles for this project's sessions so the next turn rehydrates on the new `cwd`!
        for sid in session_ids {
            let _ = self.runtime().close_session(sid).await;
        }

        Ok(ProjectLifecycleResponse {
            project: updated,
            affected_timers: 0,
            user_workspace_files_deleted: false,
        })
    }

    /// Detaches a project's workspace folder binding WITHOUT touching any files on disk
    /// (`POST /v1/projects/{id}/detach-workspace`, Contract C).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::Conflict`], or [`DennettError::DatabaseUnavailable`].
    pub async fn detach_project_workspace(
        &self,
        project_id: ProjectId,
        correlation_id: &CorrelationId,
    ) -> Result<ProjectLifecycleResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let existing = fetch_project(&tx, project_id, self.pg_target()).await?;
        if existing.lifecycle_state == ProjectLifecycleState::DeletedRecord {
            return Err(DennettError::Conflict {
                resource: String::from("project"),
                expected_revision: None,
                actual_revision: None,
                reason: String::from("cannot detach workspace on a deleted project record"),
            });
        }
        let next_lifecycle = if existing.lifecycle_state == ProjectLifecycleState::Archived {
            ProjectLifecycleState::Archived
        } else {
            ProjectLifecycleState::Detached
        };

        let sql = format!(
            "UPDATE projects
             SET workspace_path = NULL,
                 lifecycle_state = $2,
                 updated_at = now()
             WHERE project_id = $1
             RETURNING {PROJECT_SELECT_COLS}"
        );
        let row = tx
            .query_one(&sql, &[&project_id.as_uuid(), &next_lifecycle.as_str()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let updated = decode_project_row(&row)?;

        let session_rows = tx
            .query(
                "SELECT session_id FROM sessions WHERE project_id = $1",
                &[&project_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let session_ids: Vec<SessionId> = session_rows
            .iter()
            .map(|r| SessionId::from_uuid(r.get(0)))
            .collect();

        let _ = append_audit_event(
            &tx,
            self.pg_target(),
            correlation_id,
            None,
            None,
            "head",
            "project_workspace_detached",
            &format!("detached workspace binding from project {project_id} (disk files untouched)"),
        )
        .await?;

        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &format!("project:{project_id}"),
                event_type: "project.workspace_detached",
                project_id: Some(project_id),
                session_id: None,
                task_id: None,
                correlation_id,
                payload: json!({
                    "project_id": project_id,
                    "previous_workspace_path": existing.workspace_path,
                    "lifecycle_state": updated.lifecycle_state,
                    "user_workspace_files_deleted": false,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        for sid in session_ids {
            let _ = self.runtime().close_session(sid).await;
        }

        Ok(ProjectLifecycleResponse {
            project: updated,
            affected_timers: 0,
            user_workspace_files_deleted: false,
        })
    }

    /// Reversibly archives a project and pauses all pending scheduled timers belonging to it
    /// (`POST /v1/projects/{id}/archive`, Scenario 90 §30, Contract C). Never deletes disk files.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::Conflict`], or [`DennettError::DatabaseUnavailable`].
    pub async fn archive_project(
        &self,
        project_id: ProjectId,
        correlation_id: &CorrelationId,
    ) -> Result<ProjectLifecycleResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let existing = fetch_project(&tx, project_id, self.pg_target()).await?;
        if existing.lifecycle_state == ProjectLifecycleState::DeletedRecord {
            return Err(DennettError::Conflict {
                resource: String::from("project"),
                expected_revision: None,
                actual_revision: None,
                reason: String::from("cannot archive a deleted project record"),
            });
        }

        let sql = format!(
            "UPDATE projects
             SET lifecycle_state = 'archived',
                 archived_at = COALESCE(archived_at, now()),
                 updated_at = now()
             WHERE project_id = $1
             RETURNING {PROJECT_SELECT_COLS}"
        );
        let row = tx
            .query_one(&sql, &[&project_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let updated = decode_project_row(&row)?;

        // Pause all pending timers of this project's tasks so archived projects never fire silently!
        let paused_timers = tx
            .execute(
                "UPDATE task_timers t
                 SET status = 'paused'
                 FROM tasks tk
                 WHERE t.task_id = tk.task_id
                   AND tk.project_id = $1
                   AND t.status = 'pending'",
                &[&project_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let paused_usize = usize::try_from(paused_timers).unwrap_or(0);
        let _ = append_audit_event(
            &tx,
            self.pg_target(),
            correlation_id,
            None,
            None,
            "head",
            "project_archived",
            &format!("archived project {project_id} (paused_timers={paused_usize})"),
        )
        .await?;

        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &format!("project:{project_id}"),
                event_type: "project.archived",
                project_id: Some(project_id),
                session_id: None,
                task_id: None,
                correlation_id,
                payload: json!({
                    "project_id": project_id,
                    "paused_timers": paused_usize,
                    "user_workspace_files_deleted": false,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        Ok(ProjectLifecycleResponse {
            project: updated,
            affected_timers: paused_usize,
            user_workspace_files_deleted: false,
        })
    }

    /// Restores an archived project back to `active` (or `detached` when folderless) and resumes
    /// its paused timers (`POST /v1/projects/{id}/unarchive`, Scenario 90 §30).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::Conflict`], or [`DennettError::DatabaseUnavailable`].
    pub async fn unarchive_project(
        &self,
        project_id: ProjectId,
        correlation_id: &CorrelationId,
    ) -> Result<ProjectLifecycleResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let existing = fetch_project(&tx, project_id, self.pg_target()).await?;
        if existing.lifecycle_state != ProjectLifecycleState::Archived {
            return Err(DennettError::Conflict {
                resource: String::from("project"),
                expected_revision: None,
                actual_revision: None,
                reason: format!(
                    "project `{project_id}` is `{}` and not archived",
                    existing.lifecycle_state
                ),
            });
        }
        let next_state = if existing.workspace_path.is_some() {
            ProjectLifecycleState::Active
        } else {
            ProjectLifecycleState::Detached
        };

        let sql = format!(
            "UPDATE projects
             SET lifecycle_state = $2,
                 archived_at = NULL,
                 updated_at = now()
             WHERE project_id = $1
             RETURNING {PROJECT_SELECT_COLS}"
        );
        let row = tx
            .query_one(&sql, &[&project_id.as_uuid(), &next_state.as_str()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let updated = decode_project_row(&row)?;

        let resumed_timers = tx
            .execute(
                "UPDATE task_timers t
                 SET status = 'pending'
                 FROM tasks tk
                 WHERE t.task_id = tk.task_id
                   AND tk.project_id = $1
                   AND t.status = 'paused'
                   AND tk.status = 'waiting_timer'",
                &[&project_id.as_uuid()],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let resumed_usize = usize::try_from(resumed_timers).unwrap_or(0);

        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &format!("project:{project_id}"),
                event_type: "project.unarchived",
                project_id: Some(project_id),
                session_id: None,
                task_id: None,
                correlation_id,
                payload: json!({
                    "project_id": project_id,
                    "lifecycle_state": updated.lifecycle_state,
                    "resumed_timers": resumed_usize,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        Ok(ProjectLifecycleResponse {
            project: updated,
            affected_timers: resumed_usize,
            user_workspace_files_deleted: false,
        })
    }

    /// Removes a project entry from active Dennett lists WITHOUT deleting any external user files
    /// on disk (`POST /v1/projects/{id}/remove`, Contract C).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`] or [`DennettError::DatabaseUnavailable`].
    pub async fn remove_project(
        &self,
        project_id: ProjectId,
        correlation_id: &CorrelationId,
    ) -> Result<ProjectLifecycleResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let _ = fetch_project(&tx, project_id, self.pg_target()).await?;

        let sql = format!(
            "UPDATE projects
             SET lifecycle_state = 'deleted_record',
                 deleted_at = COALESCE(deleted_at, now()),
                 updated_at = now()
             WHERE project_id = $1
             RETURNING {PROJECT_SELECT_COLS}"
        );
        let row = tx
            .query_one(&sql, &[&project_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let updated = decode_project_row(&row)?;

        let work = self
            .terminate_project_work_in_tx(&tx, project_id, "Проект убран из списка Dennett")
            .await?;
        let affected = work.affected_timers;

        let _ = append_audit_event(
            &tx,
            self.pg_target(),
            correlation_id,
            None,
            None,
            "head",
            "project_removed",
            &format!("removed project {project_id} from Dennett lists (disk files untouched)"),
        )
        .await?;

        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &format!("project:{project_id}"),
                event_type: "project.removed",
                project_id: Some(project_id),
                session_id: None,
                task_id: None,
                correlation_id,
                payload: json!({
                    "project_id": project_id,
                    "cancelled_timers": affected,
                    "cancelled_tasks": work.cancelled_task_ids.len(),
                    "user_workspace_files_deleted": false,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        self.finish_project_work_shutdown(work, "project removed from Dennett")
            .await;

        Ok(ProjectLifecycleResponse {
            project: updated,
            affected_timers: affected,
            user_workspace_files_deleted: false,
        })
    }

    /// Deletes Dennett's internal scratch data and metadata record for `project_id`
    /// (`DELETE /v1/projects/{id}?confirm=true`, Contract C).
    ///
    /// STRICTLY NEVER deletes external user workspace folders on disk!
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `confirm != true`, [`DennettError::NotFound`],
    /// or [`DennettError::DatabaseUnavailable`].
    pub async fn delete_project(
        &self,
        project_id: ProjectId,
        confirm: bool,
        correlation_id: &CorrelationId,
    ) -> Result<ProjectLifecycleResponse, DennettError> {
        if !confirm {
            return Err(DennettError::InvalidInput {
                field: String::from("confirm"),
                reason: String::from(
                    "deleting Dennett project data requires explicit `?confirm=true`",
                ),
            });
        }
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let _ = fetch_project(&tx, project_id, self.pg_target()).await?;

        // Scrub inline Dennett artifact payloads for this project (never touching user disk files!).
        tx.execute(
            "UPDATE artifacts
             SET lifecycle_state = 'deleted',
                 content = NULL,
                 relative_path = NULL,
                 updated_at = now()
             WHERE project_id = $1 AND lifecycle_state <> 'deleted'",
            &[&project_id.as_uuid()],
        )
        .await
        .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let work = self
            .terminate_project_work_in_tx(&tx, project_id, "Проект удалён в Dennett")
            .await?;
        let affected = work.affected_timers;

        let sql = format!(
            "UPDATE projects
             SET workspace_path = NULL,
                 lifecycle_state = 'deleted_record',
                 deleted_at = COALESCE(deleted_at, now()),
                 updated_at = now()
             WHERE project_id = $1
             RETURNING {PROJECT_SELECT_COLS}"
        );
        let row = tx
            .query_one(&sql, &[&project_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let updated = decode_project_row(&row)?;

        let _ = append_audit_event(
            &tx,
            self.pg_target(),
            correlation_id,
            None,
            None,
            "head",
            "project_deleted",
            &format!(
                "deleted Dennett metadata/scratch for project {project_id} (external user files untouched)"
            ),
        )
        .await?;

        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &format!("project:{project_id}"),
                event_type: "project.deleted",
                project_id: Some(project_id),
                session_id: None,
                task_id: None,
                correlation_id,
                payload: json!({
                    "project_id": project_id,
                    "cancelled_timers": affected,
                    "cancelled_tasks": work.cancelled_task_ids.len(),
                    "user_workspace_files_deleted": false,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        self.finish_project_work_shutdown(work, "project deleted in Dennett")
            .await;

        // Clean ONLY Dennett's internal scratch directory (never the user's bound workspace folder!).
        let scratch_dir = self.detached_project_scratch_dir(project_id);
        if scratch_dir.exists() {
            let _ = fs::remove_dir_all(&scratch_dir);
        }

        Ok(ProjectLifecycleResponse {
            project: updated,
            affected_timers: affected,
            user_workspace_files_deleted: false,
        })
    }

    /// Returns all revisions (`v1`, `v2`, ...) of a logical artifact identified by either its
    /// [`ArtifactId`] UUID or logical `name` inside `session_id` (Contract D).
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`] or [`DennettError::DatabaseUnavailable`].
    pub async fn list_artifact_revisions(
        &self,
        session_id: SessionId,
        artifact_id_or_name: &str,
    ) -> Result<ListArtifactsResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let _ = fetch_session(&conn, session_id, self.pg_target()).await?;

        let logical_name = if let Ok(art_id) = ArtifactId::from_str(artifact_id_or_name.trim()) {
            let name_row = conn
                .query_opt(
                    "SELECT name FROM artifacts WHERE session_id = $1 AND artifact_id = $2",
                    &[&session_id.as_uuid(), &art_id.as_uuid()],
                )
                .await
                .map_err(|err| map_pg_err(self.pg_target(), &err))?;
            let Some(row) = name_row else {
                return Err(DennettError::NotFound {
                    resource: String::from("artifact"),
                    id: artifact_id_or_name.to_owned(),
                });
            };
            row.get::<_, String>(0)
        } else {
            artifact_id_or_name.trim().to_owned()
        };

        let sql = format!(
            "SELECT {ARTIFACT_SELECT_COLS}
             FROM artifacts
             WHERE session_id = $1 AND name = $2
             ORDER BY revision ASC"
        );
        let rows = conn
            .query(&sql, &[&session_id.as_uuid(), &logical_name])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        if rows.is_empty() {
            return Err(DennettError::NotFound {
                resource: String::from("artifact"),
                id: artifact_id_or_name.to_owned(),
            });
        }
        let artifacts = rows
            .iter()
            .map(decode_artifact_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ListArtifactsResponse { artifacts })
    }

    /// Applies a distinct lifecycle action (`finalize`, `archive`, `unarchive`, `remove`, `delete`)
    /// to one persisted artifact revision (`POST /v1/sessions/{session_id}/artifacts/{artifact_id}/lifecycle`,
    /// Contract D). STRICTLY NEVER deletes workspace files on disk!
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::NotFound`], [`DennettError::Conflict`], or [`DennettError::DatabaseUnavailable`].
    #[expect(
        clippy::too_many_lines,
        reason = "applies explicit Contract D artifact lifecycle transition atomically with audit and control journal events"
    )]
    pub async fn apply_artifact_lifecycle(
        &self,
        session_id: SessionId,
        artifact_id: ArtifactId,
        req: ArtifactLifecycleRequest,
        correlation_id: &CorrelationId,
    ) -> Result<ArtifactLifecycleResponse, DennettError> {
        let _ = self.ensure_migrated().await?;
        let mut conn = self.get_conn(DEFAULT_PG_OP_TIMEOUT).await?;
        let tx = conn
            .transaction()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        let lock_sql = format!(
            "SELECT {ARTIFACT_SELECT_COLS}
             FROM artifacts
             WHERE session_id = $1 AND artifact_id = $2
             FOR UPDATE"
        );
        let Some(row) = tx
            .query_opt(&lock_sql, &[&session_id.as_uuid(), &artifact_id.as_uuid()])
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?
        else {
            return Err(DennettError::NotFound {
                resource: String::from("artifact"),
                id: artifact_id.to_string(),
            });
        };
        let existing = decode_artifact_row(&row)?;

        if existing.lifecycle_state == ArtifactLifecycleState::Deleted
            && req.action != ArtifactLifecycleAction::Delete
        {
            return Err(DennettError::Conflict {
                resource: String::from("artifact"),
                expected_revision: None,
                actual_revision: None,
                reason: String::from("cannot transition a deleted artifact tombstone"),
            });
        }

        let (next_completeness, next_lifecycle, scrub_payload, event_type) = match req.action {
            ArtifactLifecycleAction::Finalize => (
                ArtifactCompleteness::Final,
                existing.lifecycle_state,
                false,
                "artifact.finalized",
            ),
            ArtifactLifecycleAction::Archive => (
                existing.completeness_status,
                ArtifactLifecycleState::Archived,
                false,
                "artifact.archived",
            ),
            ArtifactLifecycleAction::Unarchive => (
                existing.completeness_status,
                ArtifactLifecycleState::Active,
                false,
                "artifact.unarchived",
            ),
            ArtifactLifecycleAction::Remove => (
                existing.completeness_status,
                ArtifactLifecycleState::Removed,
                false,
                "artifact.removed",
            ),
            ArtifactLifecycleAction::Delete => (
                existing.completeness_status,
                ArtifactLifecycleState::Deleted,
                true,
                "artifact.deleted",
            ),
            _ => {
                return Err(DennettError::InvalidInput {
                    field: String::from("action"),
                    reason: String::from("unsupported artifact lifecycle action"),
                });
            }
        };

        let update_sql = format!(
            "UPDATE artifacts
             SET completeness_status = $3,
                 lifecycle_state = $4,
                 content = CASE WHEN $5 THEN NULL ELSE content END,
                 relative_path = CASE WHEN $5 THEN NULL ELSE relative_path END,
                 updated_at = now()
             WHERE session_id = $1 AND artifact_id = $2
             RETURNING {ARTIFACT_SELECT_COLS}"
        );
        let updated_row = tx
            .query_one(
                &update_sql,
                &[
                    &session_id.as_uuid(),
                    &artifact_id.as_uuid(),
                    &next_completeness.as_str(),
                    &next_lifecycle.as_str(),
                    &scrub_payload,
                ],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        let updated = decode_artifact_row(&updated_row)?;

        // H2-07: Removing from list hides all non-deleted revisions of this logical artifact name
        // in the session so multi-revision artifacts (`v1`+`v2`) vanish cleanly as one logical item
        // (strictly leaving external workspace files on disk untouched).
        if req.action == ArtifactLifecycleAction::Remove {
            tx.execute(
                "UPDATE artifacts
                 SET lifecycle_state = 'removed',
                     updated_at = now()
                 WHERE session_id = $1
                   AND name = $2
                   AND lifecycle_state NOT IN ('removed', 'deleted')",
                &[&session_id.as_uuid(), &updated.name],
            )
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;
        }

        let safe_reason = req
            .reason
            .as_deref()
            .map(|r| truncate_utf8(&self.redact(r), 500).to_owned());
        let _ = append_audit_event(
            &tx,
            self.pg_target(),
            correlation_id,
            Some(session_id),
            updated.run_id,
            "head",
            event_type,
            &format!(
                "artifact `{}` rev {} ({artifact_id}) -> action={} (disk files untouched)",
                updated.name, updated.revision, req.action
            ),
        )
        .await?;

        let _ = append_control_event_in_tx(
            &tx,
            self.pg_target(),
            ControlEventAppend {
                entity_key: &format!("session:{session_id}"),
                event_type,
                project_id: Some(updated.project_id),
                session_id: Some(session_id),
                task_id: None,
                correlation_id,
                payload: json!({
                    "artifact_id": artifact_id,
                    "name": updated.name,
                    "revision": updated.revision,
                    "completeness_status": updated.completeness_status,
                    "lifecycle_state": updated.lifecycle_state,
                    "reason": safe_reason,
                    "user_workspace_files_deleted": false,
                }),
            },
        )
        .await?;

        tx.commit()
            .await
            .map_err(|err| map_pg_err(self.pg_target(), &err))?;

        Ok(ArtifactLifecycleResponse {
            artifact: updated,
            user_workspace_files_deleted: false,
        })
    }
}
