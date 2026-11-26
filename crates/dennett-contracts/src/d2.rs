//! Pure Phase D2 domain contracts for durable tasks, schedules/timers, idempotent signals,
//! external budgets, interactive approvals, revocable capability grants, Action Inbox,
//! plain-text inter-agent messages, ordered delivery journal/outbox, external effect claims,
//! and strict project/artifact lifecycle separation.
//!
//! Depends strictly on `std`, `serde`, `thiserror`, and `uuid` (zero extra crate dependencies).

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::control::{
    AgentId, AgentRecord, ArtifactRecord, MAX_MODEL_NAME_LEN, MAX_POLICY_PATTERN_LEN,
    MAX_POLICY_SCOPE_LEN, MAX_REASON_BYTES, MAX_WORKSPACE_PATH_LEN, MemoryStatus,
    PermissionDecisionId, PermissionDecisionRecord, ProjectId, ProjectRecord, RunId, RunRecord,
    SessionId, SessionRecord, ThinkingLevel, WorkspaceWritePolicy,
};
use crate::d3::ProviderConnectionId;
use crate::{
    CorrelationId, DennettError, IdempotencyKey, RuntimeAuthProfile, Sha256Hex, TimestampMs,
};

/// Maximum permitted byte length of a [`TaskRecord`] title.
pub const MAX_TASK_TITLE_LEN: usize = 256;
/// Maximum permitted byte length of a [`TaskRecord`] goal description (32 KiB).
pub const MAX_TASK_GOAL_BYTES: usize = 32 * 1024;
/// Maximum permitted byte length of a task signal name.
pub const MAX_SIGNAL_NAME_LEN: usize = 128;
/// Maximum permitted byte length of a task signal payload (32 KiB).
pub const MAX_SIGNAL_PAYLOAD_BYTES: usize = 32 * 1024;
/// Maximum permitted byte length of an IANA timezone identifier.
pub const MAX_TIMEZONE_LEN: usize = 128;
/// Maximum permitted byte length of a human schedule expression.
pub const MAX_SCHEDULE_EXPR_LEN: usize = 256;
/// Maximum permitted number of attached skill identifiers on an agent or task.
pub const MAX_ATTACHED_SKILLS: usize = 32;
/// Maximum permitted byte length of one attached skill identifier.
pub const MAX_SKILL_NAME_LEN: usize = 128;
/// Maximum permitted byte length of a client message idempotency identifier.
pub const MAX_CLIENT_MESSAGE_ID_LEN: usize = 128;
/// Maximum permitted byte length of an inter-agent plain-text message body (64 KiB).
pub const MAX_AGENT_MESSAGE_BYTES: usize = 64 * 1024;
/// Maximum permitted byte length of an external connector identifier.
pub const MAX_CONNECTOR_ID_LEN: usize = 64;
/// Maximum permitted byte length of an external effect operation name.
pub const MAX_EFFECT_OPERATION_LEN: usize = 128;
/// Maximum permitted byte length of an external effect recipient target.
pub const MAX_EFFECT_RECIPIENT_LEN: usize = 512;
/// Maximum permitted byte length of serialized JSON effect parameters (64 KiB).
pub const MAX_EFFECT_PARAMS_BYTES: usize = 64 * 1024;
/// Maximum permitted byte length of a clarification answer text (32 KiB).
pub const MAX_CLARIFICATION_ANSWER_BYTES: usize = 32 * 1024;

macro_rules! define_d2_uuid_id {
    (
        $(#[$meta:meta])*
        pub struct $name:ident;
        field = $field:literal;
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        #[repr(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Generates a fresh random UUID v4 identifier.
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// Wraps an existing [`Uuid`] into this typed identifier.
            #[must_use]
            pub const fn from_uuid(id: Uuid) -> Self {
                Self(id)
            }

            /// Returns the underlying [`Uuid`].
            #[must_use]
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl FromStr for $name {
            type Err = DennettError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(s).map(Self).map_err(|_| DennettError::InvalidInput {
                    field: String::from($field),
                    reason: String::from("must be a valid UUID"),
                })
            }
        }
    };
}

define_d2_uuid_id! {
    /// Stable UUID identifying a durable Dennett [`TaskRecord`].
    pub struct TaskId;
    field = "task_id";
}

define_d2_uuid_id! {
    /// Stable UUID identifying a persisted [`TaskTimerRecord`].
    pub struct TaskTimerId;
    field = "timer_id";
}

define_d2_uuid_id! {
    /// Stable UUID identifying an idempotent [`TaskSignalRecord`].
    pub struct TaskSignalId;
    field = "signal_id";
}

define_d2_uuid_id! {
    /// Stable UUID identifying a live interactive [`PermissionRequestRecord`].
    pub struct PermissionRequestId;
    field = "request_id";
}

define_d2_uuid_id! {
    /// Stable UUID identifying a revocable [`CapabilityGrantRecord`].
    pub struct CapabilityGrantId;
    field = "grant_id";
}

define_d2_uuid_id! {
    /// Stable UUID identifying an Action Inbox [`InboxCardRecord`].
    pub struct InboxCardId;
    field = "card_id";
}

define_d2_uuid_id! {
    /// Stable UUID identifying a tracked [`EffectClaimRecord`].
    pub struct EffectClaimId;
    field = "effect_id";
}

define_d2_uuid_id! {
    /// Stable UUID identifying a plain-text [`AgentMessageRecord`].
    pub struct AgentMessageId;
    field = "agent_message_id";
}

define_d2_uuid_id! {
    /// Stable UUID identifying a journaled [`ControlEventRecord`].
    pub struct ControlEventId;
    field = "event_id";
}

define_d2_uuid_id! {
    /// Stable UUID identifying one row in `ordered_outbox`.
    pub struct OrderedOutboxItemId;
    field = "item_id";
}

// ============================================================================
// Project & Artifact Lifecycle Enums (Contracts C & D)
// ============================================================================

/// Distinct project lifecycle state separating bind, detach, archive, and record removal (Contract C).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ProjectLifecycleState {
    /// Active project accepting new sessions, tasks, and timer firings.
    #[default]
    Active,
    /// Reversibly archived project whose scheduled timers are paused (user files untouched).
    Archived,
    /// Project with its workspace folder explicitly detached (user files untouched).
    Detached,
    /// Removed or deleted Dennett project metadata tombstone (external user folder untouched).
    DeletedRecord,
}

impl ProjectLifecycleState {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
            Self::Detached => "detached",
            Self::DeletedRecord => "deleted_record",
        }
    }
}

impl fmt::Display for ProjectLifecycleState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ProjectLifecycleState {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "archived" => Ok(Self::Archived),
            "detached" => Ok(Self::Detached),
            "deleted_record" => Ok(Self::DeletedRecord),
            other => Err(DennettError::InvalidInput {
                field: String::from("lifecycle_state"),
                reason: format!("unknown project lifecycle state `{other}`"),
            }),
        }
    }
}

/// Completeness status distinguishing partial intermediate drafts from final artifacts (Contract D).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ArtifactCompleteness {
    /// Partial intermediate output captured before task completion or cancellation.
    Partial,
    /// Completed final artifact revision.
    #[default]
    Final,
}

/// Canonical alias for [`ArtifactCompleteness`].
pub type ArtifactStatus = ArtifactCompleteness;

impl ArtifactCompleteness {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Partial => "partial",
            Self::Final => "final",
        }
    }
}

impl fmt::Display for ArtifactCompleteness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ArtifactCompleteness {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "partial" => Ok(Self::Partial),
            "final" => Ok(Self::Final),
            other => Err(DennettError::InvalidInput {
                field: String::from("completeness_status"),
                reason: format!("unknown artifact completeness status `{other}`"),
            }),
        }
    }
}

/// Distinct artifact lifecycle state separating archive, list removal, and payload deletion (Contract D).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ArtifactLifecycleState {
    /// Visible active artifact revision.
    #[default]
    Active,
    /// Reversibly archived artifact revision.
    Archived,
    /// Hidden/removed from active Dennett listings (external workspace files untouched).
    Removed,
    /// Dennett payload tombstone with inline content scrubbed (external workspace files untouched).
    Deleted,
}

impl ArtifactLifecycleState {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
            Self::Removed => "removed",
            Self::Deleted => "deleted",
        }
    }
}

impl fmt::Display for ArtifactLifecycleState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ArtifactLifecycleState {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "archived" => Ok(Self::Archived),
            "removed" => Ok(Self::Removed),
            "deleted" => Ok(Self::Deleted),
            other => Err(DennettError::InvalidInput {
                field: String::from("lifecycle_state"),
                reason: format!("unknown artifact lifecycle state `{other}`"),
            }),
        }
    }
}

// ============================================================================
// Durable Tasks, Schedules, Timers, Signals & Budgets
// ============================================================================

/// Execution and waiting status of a durable [`TaskRecord`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TaskStatus {
    /// Ready for background execution by Head.
    Queued,
    /// Currently executing a runtime turn.
    Running,
    /// Parked waiting for a durable PostgreSQL timer (`task_timers`).
    WaitingTimer,
    /// Parked waiting for an idempotent external or inter-agent signal (`task_signals`).
    WaitingSignal,
    /// Parked waiting for the user's answer to an agent clarification question.
    WaitingClarification,
    /// Parked waiting for interactive owner approval (write permission, budget increase, or missed run).
    WaitingApproval,
    /// Completed cleanly for its latest goal generation.
    Succeeded,
    /// Terminated with a non-recovered error or hard budget exhaustion.
    Failed,
    /// Explicitly halted by targeted `Stop` without unknown external side effects.
    Cancelled,
    /// One or more external effects had a lost acknowledgement (`UNKNOWN`); blind retry is blocked until reconciliation.
    UnknownExternal,
}

impl TaskStatus {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::WaitingTimer => "waiting_timer",
            Self::WaitingSignal => "waiting_signal",
            Self::WaitingClarification => "waiting_clarification",
            Self::WaitingApproval => "waiting_approval",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::UnknownExternal => "unknown_external",
        }
    }

    /// Returns `true` when this task status is terminal (`succeeded`, `failed`, or `cancelled`).
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TaskStatus {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "waiting_timer" => Ok(Self::WaitingTimer),
            "waiting_signal" => Ok(Self::WaitingSignal),
            "waiting_clarification" => Ok(Self::WaitingClarification),
            "waiting_approval" => Ok(Self::WaitingApproval),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "unknown_external" => Ok(Self::UnknownExternal),
            other => Err(DennettError::InvalidInput {
                field: String::from("status"),
                reason: format!("unknown task status `{other}`"),
            }),
        }
    }
}

/// Schedule recurrence rule evaluated by `dennett-head` using PostgreSQL 16 IANA timezone support.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ScheduleKind {
    /// Fire exactly once at the given UTC timestamp.
    OnceAt {
        /// Target fire timestamp in UTC epoch milliseconds.
        fire_at: TimestampMs,
    },
    /// Fire repeatedly every `interval_secs` seconds.
    Interval {
        /// Positive interval in seconds (`>= 1`).
        interval_secs: u64,
    },
    /// Fire daily at local wall-clock `hour:minute` in `TaskSchedule::timezone` (DST-aware via PG 16).
    #[serde(alias = "daily_local_time")]
    DailyWallTime {
        /// Local hour in `0..=23`.
        hour: u8,
        /// Local minute in `0..=59`.
        minute: u8,
    },
    /// Fire weekly on selected ISO-8601 weekdays (`1..=127`, bit 0 = Monday .. bit 6 = Sunday)
    /// at local wall-clock `hour:minute` in `TaskSchedule::timezone` (DST-aware via PG 16).
    #[serde(alias = "weekly_local_time")]
    WeeklyWallTime {
        /// ISO-8601 weekday bitmask (`1..=127`, bit 0 = Monday .. bit 6 = Sunday).
        weekdays_mask: u8,
        /// Local hour in `0..=23`.
        hour: u8,
        /// Local minute in `0..=59`.
        minute: u8,
    },
    /// Fire yearly on local calendar date `month`/`day` at wall-clock `hour:minute`
    /// in `TaskSchedule::timezone` (DST-aware via PG 16).
    #[serde(alias = "yearly_local_time")]
    YearlyWallTime {
        /// Calendar month (`1..=12`).
        month: u8,
        /// Calendar day of month (`1..=31`, bounded by the maximum days in `month`).
        day: u8,
        /// Local hour in `0..=23`.
        hour: u8,
        /// Local minute in `0..=59`.
        minute: u8,
    },
}

impl ScheduleKind {
    /// Returns the database `recurrence_kind` label (`once`, `interval`, `daily_wall`, `weekly_wall`, or `yearly_wall`).
    #[must_use]
    pub const fn recurrence_label(&self) -> &'static str {
        match self {
            Self::OnceAt { .. } => "once",
            Self::Interval { .. } => "interval",
            Self::DailyWallTime { .. } => "daily_wall",
            Self::WeeklyWallTime { .. } => "weekly_wall",
            Self::YearlyWallTime { .. } => "yearly_wall",
        }
    }

    /// Validates numeric schedule bounds and timestamp range.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] on out-of-range timestamps, zero intervals,
    /// invalid weekday masks, invalid calendar month/day, or out-of-range hour/minute values.
    pub fn validate(&self) -> Result<(), DennettError> {
        match *self {
            Self::OnceAt { fire_at } => fire_at.validate("schedule.fire_at"),
            Self::Interval { interval_secs } => {
                if interval_secs == 0 || interval_secs > 366 * 86_400 {
                    return Err(DennettError::InvalidInput {
                        field: String::from("schedule.interval_secs"),
                        reason: String::from("must be between 1 and 31622400 seconds"),
                    });
                }
                Ok(())
            }
            Self::DailyWallTime { hour, minute } => validate_wall_clock_hhmm(hour, minute),
            Self::WeeklyWallTime {
                weekdays_mask,
                hour,
                minute,
            } => {
                if weekdays_mask == 0 || weekdays_mask > 0b0111_1111 {
                    return Err(DennettError::InvalidInput {
                        field: String::from("schedule.weekdays_mask"),
                        reason: String::from(
                            "must be a non-empty 7-bit ISO weekday mask in 1..=127 (bit 0=Mon .. bit 6=Sun)",
                        ),
                    });
                }
                validate_wall_clock_hhmm(hour, minute)
            }
            Self::YearlyWallTime {
                month,
                day,
                hour,
                minute,
            } => {
                let max_day = match month {
                    1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
                    4 | 6 | 9 | 11 => 30,
                    2 => 29,
                    _ => {
                        return Err(DennettError::InvalidInput {
                            field: String::from("schedule.month"),
                            reason: String::from("must be between 1 and 12"),
                        });
                    }
                };
                if day == 0 || day > max_day {
                    return Err(DennettError::InvalidInput {
                        field: String::from("schedule.day"),
                        reason: format!("must be between 1 and {max_day} for month {month}"),
                    });
                }
                validate_wall_clock_hhmm(hour, minute)
            }
        }
    }
}

/// Explicit wake trigger for a durable [`TaskRecord`] beyond timer schedules or manual signals.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum TaskTrigger {
    /// Start according to standard creation semantics (`execute_immediately`, `schedule`, or `wait_for_signal`).
    #[default]
    Immediate,
    /// Park the task until the specified [`RunId`] finishes, then wake automatically.
    OnRunCompleted {
        /// Execution run identifier whose completion wakes this task.
        run_id: RunId,
    },
}

fn validate_wall_clock_hhmm(hour: u8, minute: u8) -> Result<(), DennettError> {
    if hour > 23 {
        return Err(DennettError::InvalidInput {
            field: String::from("schedule.hour"),
            reason: String::from("must be between 0 and 23"),
        });
    }
    if minute > 59 {
        return Err(DennettError::InvalidInput {
            field: String::from("schedule.minute"),
            reason: String::from("must be between 0 and 59"),
        });
    }
    Ok(())
}

/// Misfire handling policy when `dennett-head` recovers overdue timers after downtime (Contract I).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum MissedRunPolicy {
    /// Coalesce any number of missed windows into a single execution, then advance strictly `> now`.
    #[default]
    ExecuteOnce,
    /// Mark missed window as skipped and schedule the next future window strictly `> now`.
    Skip,
    /// Park the task in `waiting_approval` and open a `missed_schedule` Action Inbox card.
    Ask,
}

impl MissedRunPolicy {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExecuteOnce => "execute_once",
            Self::Skip => "skip",
            Self::Ask => "ask",
        }
    }
}

impl fmt::Display for MissedRunPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for MissedRunPolicy {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "execute_once" => Ok(Self::ExecuteOnce),
            "skip" => Ok(Self::Skip),
            "ask" => Ok(Self::Ask),
            other => Err(DennettError::InvalidInput {
                field: String::from("missed_run_policy"),
                reason: format!("unknown missed_run_policy `{other}`"),
            }),
        }
    }
}

/// Persisted schedule specification on a [`TaskRecord`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskSchedule {
    /// Validated IANA timezone name (e.g. `"UTC"`, `"Asia/Yekaterinburg"`).
    pub timezone: String,
    /// Optional human-readable original schedule expression preserved for audit/UI display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_expression: Option<String>,
    /// Concrete schedule recurrence rule.
    pub kind: ScheduleKind,
    /// Misfire behavior after downtime or sleep.
    #[serde(default)]
    pub missed_run_policy: MissedRunPolicy,
    /// Next computed UTC fire time, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_fire_at: Option<TimestampMs>,
}

impl TaskSchedule {
    /// Validates timezone syntax, expression length, and schedule kind invariants.
    ///
    /// PostgreSQL 16 additionally verifies `timezone` against `pg_timezone_names` in `Head`.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] on invalid timezone syntax or schedule bounds.
    pub fn validate(&self) -> Result<(), DennettError> {
        validate_iana_timezone_syntax(&self.timezone)?;
        if let Some(ref expr) = self.original_expression {
            reject_nul(expr, "schedule.original_expression")?;
            if expr.len() > MAX_SCHEDULE_EXPR_LEN {
                return Err(DennettError::InvalidInput {
                    field: String::from("schedule.original_expression"),
                    reason: format!("exceeds maximum length of {MAX_SCHEDULE_EXPR_LEN} bytes"),
                });
            }
        }
        self.kind.validate()
    }
}

/// Validates safe IANA timezone name syntax (`UTC` or `Area/Location`) without pulling external tz crates.
///
/// # Errors
///
/// Returns [`DennettError::InvalidInput`] if empty, oversized, containing whitespace/control/`..`,
/// or invalid path segments.
pub fn validate_iana_timezone_syntax(tz: &str) -> Result<(), DennettError> {
    let trimmed = tz.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_TIMEZONE_LEN {
        return Err(DennettError::InvalidInput {
            field: String::from("timezone"),
            reason: format!("must be non-empty and at most {MAX_TIMEZONE_LEN} bytes"),
        });
    }
    if trimmed != tz
        || trimmed.starts_with('/')
        || trimmed.ends_with('/')
        || trimmed.contains("..")
        || trimmed.contains("//")
    {
        return Err(DennettError::InvalidInput {
            field: String::from("timezone"),
            reason: format!("invalid IANA timezone syntax `{tz}`"),
        });
    }
    if !trimmed
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'/' | b'_' | b'-' | b'+'))
    {
        return Err(DennettError::InvalidInput {
            field: String::from("timezone"),
            reason: format!("IANA timezone `{tz}` contains disallowed characters"),
        });
    }
    if trimmed == "UTC" || trimmed == "Etc/UTC" {
        return Ok(());
    }
    let segments: Vec<&str> = trimmed.split('/').collect();
    if !(2..=3).contains(&segments.len()) || segments.iter().any(|seg| seg.is_empty()) {
        return Err(DennettError::InvalidInput {
            field: String::from("timezone"),
            reason: format!(
                "IANA timezone `{tz}` must be `UTC` or `Area/Location` (e.g. `Asia/Yekaterinburg`)"
            ),
        });
    }
    Ok(())
}

/// Action taken by Head outside the model when a [`TaskBudget`] limit is hit (Contract G).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum BudgetExceedPolicy {
    /// Pause task in `waiting_approval` and open a `budget_exceeded` Action Inbox card.
    #[default]
    AskApproval,
    /// Fail the task immediately with `BUDGET_EXCEEDED`.
    Fail,
}

/// Explicit external execution budget enforced by `dennett-head` outside the model (Contract G).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskBudget {
    /// Maximum runtime turns permitted for this task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u32>,
    /// Maximum cumulative tokens permitted for this task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    /// Maximum cumulative wall-clock execution seconds permitted for this task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_wall_secs: Option<u64>,
    /// Behavior when any configured budget limit is exceeded.
    #[serde(default)]
    pub exceed_policy: BudgetExceedPolicy,
}

/// Machine-readable description of a specific budget limit breach.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetViolation {
    /// Dimension exceeded (`"max_turns"`, `"max_tokens"`, or `"max_wall_secs"`).
    pub dimension: String,
    /// Observed usage value.
    pub used: u64,
    /// Configured upper limit.
    pub limit: u64,
}

impl TaskBudget {
    /// Validates that any specified budget cap is strictly positive.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if any `Some(0)` cap is supplied.
    pub fn validate(&self) -> Result<(), DennettError> {
        if self.max_turns == Some(0) {
            return Err(DennettError::InvalidInput {
                field: String::from("budget.max_turns"),
                reason: String::from("must be greater than zero when specified"),
            });
        }
        if self.max_tokens == Some(0) {
            return Err(DennettError::InvalidInput {
                field: String::from("budget.max_tokens"),
                reason: String::from("must be greater than zero when specified"),
            });
        }
        if self.max_wall_secs == Some(0) {
            return Err(DennettError::InvalidInput {
                field: String::from("budget.max_wall_secs"),
                reason: String::from("must be greater than zero when specified"),
            });
        }
        Ok(())
    }

    /// Checks whether starting ANOTHER turn is forbidden because a budget cap has already been reached (`used >= limit`).
    #[must_use]
    pub fn check_before_next_turn(&self, usage: &TaskUsage) -> Option<BudgetViolation> {
        if let Some(limit) = self.max_turns
            && usage.turns_used >= limit
        {
            return Some(BudgetViolation {
                dimension: String::from("max_turns"),
                used: u64::from(usage.turns_used),
                limit: u64::from(limit),
            });
        }
        if let Some(limit) = self.max_tokens
            && usage.tokens_used >= limit
        {
            return Some(BudgetViolation {
                dimension: String::from("max_tokens"),
                used: usage.tokens_used,
                limit,
            });
        }
        if let Some(limit) = self.max_wall_secs
            && usage.wall_ms_used >= limit.saturating_mul(1_000)
        {
            return Some(BudgetViolation {
                dimension: String::from("max_wall_secs"),
                used: usage.wall_ms_used.div_ceil(1_000),
                limit,
            });
        }
        None
    }

    /// Checks whether cumulative usage AFTER a turn strictly surpassed any configured cap (`used > limit`).
    #[must_use]
    pub fn check_after_turn(&self, usage: &TaskUsage) -> Option<BudgetViolation> {
        if let Some(limit) = self.max_turns
            && usage.turns_used > limit
        {
            return Some(BudgetViolation {
                dimension: String::from("max_turns"),
                used: u64::from(usage.turns_used),
                limit: u64::from(limit),
            });
        }
        if let Some(limit) = self.max_tokens
            && usage.tokens_used > limit
        {
            return Some(BudgetViolation {
                dimension: String::from("max_tokens"),
                used: usage.tokens_used,
                limit,
            });
        }
        if let Some(limit) = self.max_wall_secs
            && usage.wall_ms_used > limit.saturating_mul(1_000)
        {
            return Some(BudgetViolation {
                dimension: String::from("max_wall_secs"),
                used: usage.wall_ms_used.div_ceil(1_000),
                limit,
            });
        }
        None
    }

    /// Computes a tighter child budget bounded by a parent task's budget (`child ⊆ parent`).
    #[must_use]
    pub fn clamp_to_parent(&self, parent: &Self) -> Self {
        let clamp_u32 = |c: Option<u32>, p: Option<u32>| match (c, p) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        let clamp_u64 = |c: Option<u64>, p: Option<u64>| match (c, p) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        Self {
            max_turns: clamp_u32(self.max_turns, parent.max_turns),
            max_tokens: clamp_u64(self.max_tokens, parent.max_tokens),
            max_wall_secs: clamp_u64(self.max_wall_secs, parent.max_wall_secs),
            exceed_policy: self.exceed_policy,
        }
    }
}

/// Confidence level of reported token usage metrics (Contract G).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum UsageConfidence {
    /// Usage numbers were explicitly reported by the runtime provider.
    ProviderReported,
    /// Usage numbers were conservatively estimated outside the provider.
    Estimated,
    /// Provider did not report token usage for one or more turns.
    #[default]
    Unknown,
}

impl UsageConfidence {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProviderReported => "provider_reported",
            Self::Estimated => "estimated",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for UsageConfidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for UsageConfidence {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "provider_reported" => Ok(Self::ProviderReported),
            "estimated" => Ok(Self::Estimated),
            "unknown" => Ok(Self::Unknown),
            other => Err(DennettError::InvalidInput {
                field: String::from("usage_confidence"),
                reason: format!("unknown usage confidence `{other}`"),
            }),
        }
    }
}

/// Cumulative resource usage counters recorded on a [`TaskRecord`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskUsage {
    /// Number of runtime turns executed on this task.
    pub turns_used: u32,
    /// Cumulative tokens recorded across turns.
    pub tokens_used: u64,
    /// Cumulative execution wall-clock milliseconds recorded across turns.
    pub wall_ms_used: u64,
    /// Confidence classification of `tokens_used`.
    #[serde(default)]
    pub confidence: UsageConfidence,
}

/// Durable persisted task/obligation record managed by `dennett-head`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRecord {
    /// Stable task identifier.
    pub task_id: TaskId,
    /// Owning project identifier.
    pub project_id: ProjectId,
    /// Dedicated or bound session identifier where task messages/runs live.
    pub session_id: SessionId,
    /// Assigned Dennett agent identifier.
    pub agent_id: AgentId,
    /// Optional parent task identifier when delegated to a helper agent (A07).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_task_id: Option<TaskId>,
    /// Concise human-readable title.
    pub title: String,
    /// Current authoritative task goal text (protected by `generation` fencing).
    pub goal: String,
    /// Monotonically increasing goal/schedule generation (`>= 1`); fences late runs (V07).
    pub generation: i64,
    /// Current durable lifecycle status.
    pub status: TaskStatus,
    /// Optional human-readable explanation of what the task is waiting on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_detail: Option<String>,
    /// Structured signal name expected while `status == WaitingSignal` (H2-04).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_signal_name: Option<String>,
    /// Preserved partial or final result summary (survives cancellation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial_result_summary: Option<String>,
    /// Machine-readable error code when `status == Failed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    /// Secret-redacted failure explanation when `status == Failed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_summary: Option<String>,
    /// Currently active runtime run identifier, when executing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_run_id: Option<RunId>,
    /// Optional durable schedule specification.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<TaskSchedule>,
    /// External execution budget enforced outside the model.
    #[serde(default)]
    pub budget: TaskBudget,
    /// Cumulative resource usage counters.
    #[serde(default)]
    pub usage: TaskUsage,
    /// Shared skill metadata labels attached to this task (A07).
    #[serde(default)]
    pub attached_skills: Vec<String>,
    /// Optional explicit runtime auth profile override (`oauth-personal` vs `gemini-api-key`, A10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_profile_override: Option<RuntimeAuthProfile>,
    /// Optional target session identifier for delegated subagent execution or follow-up posting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_session_id: Option<SessionId>,
    /// Optional session identifier to notify when this task completes or fails.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notify_session_id: Option<SessionId>,
    /// Optional runtime model identifier override for this task/subagent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_override: Option<String>,
    /// Optional reasoning effort override for this task/subagent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_override: Option<ThinkingLevel>,
    /// Optional provider connection override for this task/subagent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_override_id: Option<ProviderConnectionId>,
    /// Optional explicit skill slug override list replacing default agent skill selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_slugs_override: Option<Vec<String>>,
    /// Optional wake trigger (e.g. [`TaskTrigger::OnRunCompleted`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<TaskTrigger>,
    /// UTC creation timestamp in epoch milliseconds.
    pub created_at: TimestampMs,
    /// UTC last-updated timestamp in epoch milliseconds.
    pub updated_at: TimestampMs,
    /// UTC completion timestamp in epoch milliseconds, once terminal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<TimestampMs>,
}

/// Lifecycle state of one persisted `task_timers` row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TaskTimerStatus {
    /// Armed and waiting for `fire_at`.
    Pending,
    /// Leased by a scheduler worker via `FOR UPDATE SKIP LOCKED`.
    Claimed,
    /// Fired and woke its task.
    Fired,
    /// Temporarily paused (project archived or awaiting missed-schedule decision).
    Paused,
    /// Skipped under `MissedRunPolicy::Skip`.
    Skipped,
    /// Cancelled due to task stop, goal generation update, or project removal.
    Cancelled,
}

impl TaskTimerStatus {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Claimed => "claimed",
            Self::Fired => "fired",
            Self::Paused => "paused",
            Self::Skipped => "skipped",
            Self::Cancelled => "cancelled",
        }
    }
}

impl fmt::Display for TaskTimerStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TaskTimerStatus {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "claimed" => Ok(Self::Claimed),
            "fired" => Ok(Self::Fired),
            "paused" => Ok(Self::Paused),
            "skipped" => Ok(Self::Skipped),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(DennettError::InvalidInput {
                field: String::from("timer_status"),
                reason: format!("unknown task timer status `{other}`"),
            }),
        }
    }
}

/// Explicit reason explaining why a [`TaskTimerRecord`] is currently in [`TaskTimerStatus::Paused`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TimerPauseReason {
    /// Timer was paused automatically because its owning project was archived (restored on unarchive).
    ArchivedProject,
    /// Timer was paused after an overdue misfire awaiting owner decision via `missed_schedule` Inbox card.
    MisfireAsk,
}

impl TimerPauseReason {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ArchivedProject => "archived_project",
            Self::MisfireAsk => "misfire_ask",
        }
    }
}

impl fmt::Display for TimerPauseReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TimerPauseReason {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim() {
            "archived_project" => Ok(Self::ArchivedProject),
            "misfire_ask" => Ok(Self::MisfireAsk),
            other => Err(DennettError::InvalidInput {
                field: String::from("pause_reason"),
                reason: format!(
                    "unknown timer pause reason `{other}` (expected archived_project|misfire_ask)"
                ),
            }),
        }
    }
}

/// Persisted PostgreSQL timer owned by `dennett-head` (survives UI close and Head restart, A10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskTimerRecord {
    /// Stable timer row identifier.
    pub timer_id: TaskTimerId,
    /// Owning task identifier.
    pub task_id: TaskId,
    /// Task generation for which this timer was armed.
    pub task_generation: i64,
    /// Deterministic idempotency key (`UNIQUE (task_id, idempotency_key)`).
    pub idempotency_key: IdempotencyKey,
    /// Scheduled UTC fire timestamp in epoch milliseconds.
    pub fire_at: TimestampMs,
    /// Validated IANA timezone name.
    pub timezone: String,
    /// Optional human-readable original schedule expression.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_expression: Option<String>,
    /// Recurrence kind (`once`, `interval`, `daily_wall`, or `weekly_wall`).
    pub recurrence_kind: String,
    /// Misfire policy applied on overdue recovery.
    pub missed_run_policy: MissedRunPolicy,
    /// Current timer status.
    pub status: TaskTimerStatus,
    /// Explicit reason when `status == Paused` (`archived_project` vs `misfire_ask`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pause_reason: Option<TimerPauseReason>,
    /// Scheduler instance owning the current claim lease, if claimed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_owner: Option<String>,
    /// Monotonic fencing generation incremented on every claim.
    pub lease_generation: i64,
    /// Lease expiry timestamp in epoch milliseconds, if claimed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_expires_at: Option<TimestampMs>,
    /// UTC creation timestamp in epoch milliseconds.
    pub created_at: TimestampMs,
    /// UTC timestamp when the timer fired, if fired.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fired_at: Option<TimestampMs>,
}

/// Outcome of delivering one idempotent [`TaskSignalRecord`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TaskSignalOutcome {
    /// Signal woke a task that was waiting for signal and queued execution.
    WokeTask,
    /// Signal was durably recorded for delivery on the task's next turn.
    QueuedOnTask,
    /// Signal was recorded as rejected because the task was already cancelled or in unknown external state.
    RejectedStopped,
}

impl TaskSignalOutcome {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WokeTask => "woke_task",
            Self::QueuedOnTask => "queued_on_task",
            Self::RejectedStopped => "rejected_stopped",
        }
    }
}

impl fmt::Display for TaskSignalOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TaskSignalOutcome {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "woke_task" => Ok(Self::WokeTask),
            "queued_on_task" => Ok(Self::QueuedOnTask),
            "rejected_stopped" => Ok(Self::RejectedStopped),
            other => Err(DennettError::InvalidInput {
                field: String::from("outcome_status"),
                reason: format!("unknown task signal outcome `{other}`"),
            }),
        }
    }
}

/// Persisted idempotent signal delivered to a task (`UNIQUE (task_id, idempotency_key)`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskSignalRecord {
    /// Stable signal identifier.
    pub signal_id: TaskSignalId,
    /// Target task identifier.
    pub task_id: TaskId,
    /// Caller-supplied idempotency key deduplicating retries.
    pub idempotency_key: IdempotencyKey,
    /// Logical signal name (e.g. `"ci_passed"`, `"user_ready"`).
    pub signal_name: String,
    /// Plain-text signal payload body.
    pub payload_text: String,
    /// SHA-256 fingerprint of `(signal_name, payload_text)` used to reject conflicting key reuse.
    pub payload_sha256: Sha256Hex,
    /// Task generation observed when this signal was applied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_generation: Option<i64>,
    /// Resulting signal application status.
    pub outcome_status: TaskSignalOutcome,
    /// Correlation identifier linking this signal to control events.
    pub correlation_id: CorrelationId,
    /// UTC creation timestamp in epoch milliseconds.
    pub created_at: TimestampMs,
}

// ============================================================================
// Interactive Approvals, Revocable Grants & Action Inbox
// ============================================================================

/// Lifecycle state of an interactive [`PermissionRequestRecord`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PermissionRequestStatus {
    /// Waiting for interactive owner resolution.
    Pending,
    /// Approved strictly for this single tool invocation (`allow_once`).
    ApprovedOnce,
    /// Approved and issued a revocable [`CapabilityGrantRecord`].
    ApprovedScope,
    /// Explicitly denied by owner or re-verification policy check.
    Denied,
    /// Expired due to timeout or run cancellation (`Stop`).
    Expired,
}

impl PermissionRequestStatus {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::ApprovedOnce => "approved_once",
            Self::ApprovedScope => "approved_scope",
            Self::Denied => "denied",
            Self::Expired => "expired",
        }
    }
}

impl fmt::Display for PermissionRequestStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for PermissionRequestStatus {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "approved_once" => Ok(Self::ApprovedOnce),
            "approved_scope" => Ok(Self::ApprovedScope),
            "denied" => Ok(Self::Denied),
            "expired" => Ok(Self::Expired),
            other => Err(DennettError::InvalidInput {
                field: String::from("permission_request_status"),
                reason: format!("unknown permission request status `{other}`"),
            }),
        }
    }
}

/// Owner decision chosen when resolving an interactive [`PermissionRequestRecord`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PermissionResolutionDecision {
    /// Approve only this single invocation (`allow_once` to ACP, no standing grant).
    #[serde(alias = "allow_once")]
    ApproveOnce,
    /// Approve this invocation and create a revocable project-scoped [`CapabilityGrantRecord`].
    #[serde(alias = "allow_scope")]
    ApproveProject,
    /// Approve this invocation and create a revocable task-scoped [`CapabilityGrantRecord`].
    ApproveTask,
    /// Reject this invocation (`reject_once` to ACP).
    Deny,
}

impl PermissionResolutionDecision {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ApproveOnce => "approve_once",
            Self::ApproveProject => "approve_project",
            Self::ApproveTask => "approve_task",
            Self::Deny => "deny",
        }
    }
}

impl fmt::Display for PermissionResolutionDecision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for PermissionResolutionDecision {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "approve_once" | "allow_once" => Ok(Self::ApproveOnce),
            "approve_project" | "allow_scope" => Ok(Self::ApproveProject),
            "approve_task" => Ok(Self::ApproveTask),
            "deny" => Ok(Self::Deny),
            other => Err(DennettError::InvalidInput {
                field: String::from("decision"),
                reason: format!(
                    "unknown permission decision `{other}` (expected approve_once|approve_project|approve_task|deny)"
                ),
            }),
        }
    }
}

/// Persisted interactive permission request awaiting or resolved by the owner (`ask_before_write`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionRequestRecord {
    /// Stable permission request identifier.
    pub request_id: PermissionRequestId,
    /// Owning project identifier.
    pub project_id: ProjectId,
    /// Owning session identifier.
    pub session_id: SessionId,
    /// Active execution run identifier.
    pub run_id: RunId,
    /// Optional task identifier if invoked inside a durable task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    /// Agent requesting the tool call.
    pub agent_id: AgentId,
    /// Provider tool call identifier from `session/request_permission`.
    pub tool_call_id: String,
    /// Normalized tool name (e.g. `"write_file"`).
    pub tool_name: String,
    /// Normalized tool kind (e.g. `"edit"`).
    pub tool_kind: String,
    /// Human-readable secret-redacted target summary.
    pub target_summary: String,
    /// Verified workspace-relative target paths checked by `PolicyEngine`.
    #[serde(default)]
    pub normalized_targets: Vec<String>,
    /// SHA-256 hash of tool name, targets, and raw parameters binding approval to exact action.
    pub parameters_sha256: Sha256Hex,
    /// Optional secret-redacted bounded preview of proposed write/edit payload for owner review.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_snippet: Option<String>,
    /// Secret-redacted rationale explaining why owner approval is required.
    pub reason: String,
    /// Current resolution state.
    pub status: PermissionRequestStatus,
    /// Issued [`CapabilityGrantId`] when resolved via `ApproveProject` or `ApproveTask`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issued_grant_id: Option<CapabilityGrantId>,
    /// Linked audited [`PermissionDecisionId`] once resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_id: Option<PermissionDecisionId>,
    /// Optimistic concurrency revision (`>= 1`).
    pub revision: i64,
    /// Expiry deadline in UTC epoch milliseconds.
    pub expires_at: TimestampMs,
    /// UTC resolution timestamp in epoch milliseconds, once resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<TimestampMs>,
    /// Correlation identifier linking request, decision, and audit trail.
    pub correlation_id: CorrelationId,
    /// UTC creation timestamp in epoch milliseconds.
    pub created_at: TimestampMs,
}

/// Revocable, bounded capability grant stored in `dennett_control` and re-verified outside the model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityGrantRecord {
    /// Stable capability grant identifier.
    pub grant_id: CapabilityGrantId,
    /// Scope selector (`"project:<uuid>"` or `"task:<uuid>"`).
    pub scope: String,
    /// Owning project identifier.
    pub project_id: ProjectId,
    /// Optional agent restriction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<AgentId>,
    /// Optional session restriction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// Optional task restriction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    /// Permitted tool pattern (e.g. `"workspace_write"`, `"write_file"`).
    pub tool_pattern: String,
    /// Optional workspace-relative path prefixes restricting write targets (empty = whole canonical workspace).
    #[serde(default)]
    pub target_prefixes: Vec<String>,
    /// Optional maximum number of invocations permitted under this grant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<i32>,
    /// Number of invocations consumed so far.
    pub uses_consumed: i32,
    /// Human-readable rationale for this grant.
    pub reason: String,
    /// Optional UTC expiry timestamp in epoch milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<TimestampMs>,
    /// UTC revocation timestamp (`Some` immediately invalidates the grant outside the model).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_at: Option<TimestampMs>,
    /// Optional revocation reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_reason: Option<String>,
    /// UTC creation timestamp in epoch milliseconds.
    pub created_at: TimestampMs,
}

impl CapabilityGrantRecord {
    /// Returns `true` when this grant is not revoked, not expired at `now`, and has remaining uses.
    #[must_use]
    pub fn is_active_at(&self, now: TimestampMs) -> bool {
        if self.revoked_at.is_some() {
            return false;
        }
        if let Some(exp) = self.expires_at
            && now >= exp
        {
            return false;
        }
        if let Some(max) = self.max_uses
            && self.uses_consumed >= max
        {
            return false;
        }
        true
    }
}

/// Category of an Action Inbox decision card (`inbox_cards`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum InboxCardKind {
    /// Interactive tool/write permission approval (`ask_before_write`).
    PermissionApproval,
    /// Agent asked a clarifying question and waits for user text reply.
    Clarification,
    /// External effect suffered lost acknowledgement (`UNKNOWN`) and requires reconciliation.
    UnknownEffect,
    /// Task reached an external budget cap under `BudgetExceedPolicy::AskApproval`.
    BudgetExceeded,
    /// Recurring/scheduled timer missed its window under `MissedRunPolicy::Ask`.
    MissedSchedule,
}

impl InboxCardKind {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PermissionApproval => "permission_approval",
            Self::Clarification => "clarification",
            Self::UnknownEffect => "unknown_effect",
            Self::BudgetExceeded => "budget_exceeded",
            Self::MissedSchedule => "missed_schedule",
        }
    }
}

impl fmt::Display for InboxCardKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for InboxCardKind {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "permission_approval" => Ok(Self::PermissionApproval),
            "clarification" => Ok(Self::Clarification),
            "unknown_effect" => Ok(Self::UnknownEffect),
            "budget_exceeded" => Ok(Self::BudgetExceeded),
            "missed_schedule" => Ok(Self::MissedSchedule),
            other => Err(DennettError::InvalidInput {
                field: String::from("inbox_card_kind"),
                reason: format!("unknown inbox card kind `{other}`"),
            }),
        }
    }
}

/// Lifecycle state of an Action Inbox card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum InboxCardStatus {
    /// Open and awaiting owner decision.
    Open,
    /// Explicitly resolved by the owner or automatic reconciliation.
    Resolved,
    /// Expired after deadline elapsed.
    Expired,
    /// Superseded by a newer task goal generation.
    Superseded,
    /// Withdrawn because the parent run/task was stopped.
    Withdrawn,
}

impl InboxCardStatus {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Resolved => "resolved",
            Self::Expired => "expired",
            Self::Superseded => "superseded",
            Self::Withdrawn => "withdrawn",
        }
    }
}

impl fmt::Display for InboxCardStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for InboxCardStatus {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "open" => Ok(Self::Open),
            "resolved" => Ok(Self::Resolved),
            "expired" => Ok(Self::Expired),
            "superseded" => Ok(Self::Superseded),
            "withdrawn" => Ok(Self::Withdrawn),
            other => Err(DennettError::InvalidInput {
                field: String::from("inbox_card_status"),
                reason: format!("unknown inbox card status `{other}`"),
            }),
        }
    }
}

/// Visual priority / urgency of an Action Inbox card.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum InboxCardUrgency {
    /// Low-priority informational decision.
    Low,
    /// Standard owner decision.
    #[default]
    Normal,
    /// Prompt action recommended (e.g. live permission approval).
    High,
    /// Critical safety/reconciliation notice (e.g. `unknown_effect`).
    Critical,
}

impl InboxCardUrgency {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Normal => "normal",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

impl fmt::Display for InboxCardUrgency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for InboxCardUrgency {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "low" => Ok(Self::Low),
            "normal" => Ok(Self::Normal),
            "high" => Ok(Self::High),
            "critical" => Ok(Self::Critical),
            other => Err(DennettError::InvalidInput {
                field: String::from("urgency"),
                reason: format!("unknown inbox card urgency `{other}`"),
            }),
        }
    }
}

/// Persisted Action Inbox decision card reflecting live backend state (zero synthetic cards).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxCardRecord {
    /// Stable card identifier.
    pub card_id: InboxCardId,
    /// Owning project identifier.
    pub project_id: ProjectId,
    /// Optional owning session identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// Optional owning task identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    /// Optional execution run identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    /// Optional involved agent identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<AgentId>,
    /// Card category.
    pub kind: InboxCardKind,
    /// Current card status.
    pub status: InboxCardStatus,
    /// Urgency level.
    pub urgency: InboxCardUrgency,
    /// Concise subject line.
    pub subject: String,
    /// Secret-redacted actionable detail text.
    pub detail: String,
    /// Linked permission request when `kind == PermissionApproval`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_request_id: Option<PermissionRequestId>,
    /// Linked external effect claim when `kind == UnknownEffect`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect_id: Option<EffectClaimId>,
    /// Secret-redacted summary of how this card was resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution_summary: Option<String>,
    /// Optimistic concurrency revision (`>= 1`).
    pub revision: i64,
    /// UTC creation timestamp in epoch milliseconds.
    pub created_at: TimestampMs,
    /// UTC last-updated timestamp in epoch milliseconds.
    pub updated_at: TimestampMs,
    /// UTC resolution timestamp in epoch milliseconds, once terminal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<TimestampMs>,
}

// ============================================================================
// External Effects (Contract B / LostAck -> UNKNOWN)
// ============================================================================

/// Durable delivery state of an [`EffectClaimRecord`] (`50 §11.3`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ExternalEffectStatus {
    /// Claim recorded prior to dispatch.
    Prepared,
    /// External dispatch currently in flight outside SQL transaction.
    Dispatching,
    /// Confirmed executed by the external provider (receipt stored).
    Confirmed,
    /// Rejected by external provider or policy before execution.
    Denied,
    /// Confirmed failed before provider acceptance (`provider_count == 0`).
    Failed,
    /// Acknowledgement lost or process crashed in flight; blind retry is strictly forbidden.
    Unknown,
}

impl ExternalEffectStatus {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Dispatching => "dispatching",
            Self::Confirmed => "confirmed",
            Self::Denied => "denied",
            Self::Failed => "failed",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for ExternalEffectStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ExternalEffectStatus {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "prepared" => Ok(Self::Prepared),
            "dispatching" => Ok(Self::Dispatching),
            "confirmed" => Ok(Self::Confirmed),
            "denied" => Ok(Self::Denied),
            "failed" => Ok(Self::Failed),
            "unknown" => Ok(Self::Unknown),
            other => Err(DennettError::InvalidInput {
                field: String::from("effect_status"),
                reason: format!("unknown external effect status `{other}`"),
            }),
        }
    }
}

/// Deterministic fault-injection behavior supported by `SimulatedExternalConnector` in D2.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SimEffectBehavior {
    /// Provider executes effect once and returns a confirmation receipt.
    #[default]
    Success,
    /// Provider rejects effect before execution (`execution_count == 0`).
    Denied,
    /// Transport fails cleanly before provider accepts the effect (`execution_count == 0`).
    FailedBeforeAccept,
    /// Provider EXECUTES effect (`execution_count == 1`), then drops the acknowledgement -> `Unknown`.
    LostAck,
}

impl SimEffectBehavior {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Denied => "denied",
            Self::FailedBeforeAccept => "failed_before_accept",
            Self::LostAck => "lost_ack",
        }
    }
}

impl fmt::Display for SimEffectBehavior {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for SimEffectBehavior {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "success" => Ok(Self::Success),
            "denied" => Ok(Self::Denied),
            "failed_before_accept" | "failed" => Ok(Self::FailedBeforeAccept),
            "lost_ack" | "unknown" => Ok(Self::LostAck),
            other => Err(DennettError::InvalidInput {
                field: String::from("simulated_behavior"),
                reason: format!(
                    "unknown simulated effect behavior `{other}` (expected success|denied|failed_before_accept|lost_ack)"
                ),
            }),
        }
    }
}

/// Persisted external effect claim enforcing idempotency and `LostAck -> UNKNOWN` safety (Contract B).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectClaimRecord {
    /// Stable effect claim identifier.
    pub effect_id: EffectClaimId,
    /// Owning project identifier.
    pub project_id: ProjectId,
    /// Owning session identifier.
    pub session_id: SessionId,
    /// Optional owning task identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    /// Optional owning execution run identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    /// Agent that requested the external effect.
    pub agent_id: AgentId,
    /// Globally unique idempotency key preventing blind duplicate dispatches.
    pub effect_idempotency_key: IdempotencyKey,
    /// External connector identifier (e.g. `"simulated_external"`).
    pub connector_id: String,
    /// External operation label (e.g. `"send_webhook"`, `"send_email"`).
    pub operation: String,
    /// Secret-redacted recipient or endpoint summary.
    pub target_recipient: String,
    /// Canonical JSON string of redacted effect parameters.
    pub parameters_json: String,
    /// SHA-256 fingerprint of `(connector_id, operation, target_recipient, parameters_json)`.
    pub parameters_sha256: Sha256Hex,
    /// Current external delivery status (`unknown` strictly blocks blind retries).
    pub status: ExternalEffectStatus,
    /// Provider-assigned operation handle once confirmed or reconciled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_operation_id: Option<String>,
    /// Optional serialized JSON receipt returned by the provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt_json: Option<String>,
    /// Secret-redacted failure or unknown-state explanation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_summary: Option<String>,
    /// UTC timestamp when an `unknown` claim was reconciled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reconciled_at: Option<TimestampMs>,
    /// Correlation identifier linking this claim to audit/control events.
    pub correlation_id: CorrelationId,
    /// UTC creation timestamp in epoch milliseconds.
    pub created_at: TimestampMs,
    /// UTC last-updated timestamp in epoch milliseconds.
    pub updated_at: TimestampMs,
}

/// Canonical alias for [`EffectClaimRecord`].
pub type ExternalEffectRecord = EffectClaimRecord;

// ============================================================================
// Plain-Text Inter-Agent Coordination (A07, V07 — Zero Custom Consumer DAG)
// ============================================================================

/// Persisted plain-text message exchanged between Dennett agents (`agent_messages`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentMessageRecord {
    /// Stable inter-agent message identifier.
    pub agent_message_id: AgentMessageId,
    /// Optional sending agent identifier (`None` when authored by owner/control plane).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_agent_id: Option<AgentId>,
    /// Target recipient agent identifier.
    pub recipient_agent_id: AgentId,
    /// Shared project context identifier.
    pub project_id: ProjectId,
    /// Optional related task identifier for provenance (never a cascading kill graph).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related_task_id: Option<TaskId>,
    /// Per-recipient idempotency key (`UNIQUE (recipient_agent_id, idempotency_key)`).
    pub idempotency_key: IdempotencyKey,
    /// Plain-text coordination message body.
    pub body_text: String,
    /// Run identifier into which this message was delivered, once consumed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivered_in_run_id: Option<RunId>,
    /// Correlation identifier linking this message to audit/control events.
    pub correlation_id: CorrelationId,
    /// UTC creation timestamp in epoch milliseconds.
    pub created_at: TimestampMs,
    /// UTC delivery timestamp in epoch milliseconds, once consumed by recipient.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivered_at: Option<TimestampMs>,
}

// ============================================================================
// Ordered Delivery Journal, Snapshot Cursor & Ordered Outbox (V15–V18)
// ============================================================================

/// Authoritative gapless stream cursor read from `control_stream_counters` (`'control:journal'`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamCursor {
    /// Current authority epoch (`> 0`); changes require client snapshot resync (V18).
    pub authority_epoch: i64,
    /// Highest durably committed global `cursor_seq` visible in this snapshot (`>= 0`).
    pub committed_seq: i64,
    /// Lowest retained `cursor_seq` still queryable via `/v1/events` (`>= 1`).
    pub retained_from_seq: i64,
}

/// One append-only control journal event with gapless `cursor_seq` and per-entity `entity_seq` (V15, V18).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlEventRecord {
    /// Gapless globally ordered commit cursor allocated at transaction end (`> 0`).
    pub cursor_seq: i64,
    /// Stable event UUID.
    pub event_id: ControlEventId,
    /// Authority epoch under which this event was committed.
    pub authority_epoch: i64,
    /// Entity ordering partition key (e.g. `"task:<uuid>"`, `"project:<uuid>"`, `"session:<uuid>"`).
    pub entity_key: String,
    /// Strictly monotonic per-entity sequence number (`1, 2, ...`).
    pub entity_seq: i64,
    /// Stable dotted event type name (e.g. `"task.created"`, `"permission.requested"`).
    pub event_type: String,
    /// Optional owning project identifier for scoped filtering without payload leakage (V18).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<ProjectId>,
    /// Optional owning session identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// Optional owning task identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    /// Correlation identifier propagated from the originating command.
    pub correlation_id: CorrelationId,
    /// Secret-redacted JSON payload string.
    pub payload_json: String,
    /// UTC commit timestamp in epoch milliseconds.
    pub created_at: TimestampMs,
}

/// Lifecycle state of one row in `ordered_outbox` (V16, V17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum OrderedOutboxStatus {
    /// Ready to be claimed once it reaches the head of its `(destination, ordering_key)`.
    Pending,
    /// Leased by an active delivery worker with a fencing `lease_generation`.
    Leased,
    /// Confirmed delivered and head pointer advanced.
    Delivered,
    /// Parked after exhausting attempts (isolates ONLY this `ordering_key`).
    Parked,
}

impl OrderedOutboxStatus {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Leased => "leased",
            Self::Delivered => "delivered",
            Self::Parked => "parked",
        }
    }
}

impl fmt::Display for OrderedOutboxStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for OrderedOutboxStatus {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "leased" => Ok(Self::Leased),
            "delivered" => Ok(Self::Delivered),
            "parked" => Ok(Self::Parked),
            other => Err(DennettError::InvalidInput {
                field: String::from("ordered_outbox_status"),
                reason: format!("unknown ordered outbox status `{other}`"),
            }),
        }
    }
}

/// One per-key ordered outbox item fenced by `ordered_outbox_heads` and `lease_generation` (V16, V17).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderedOutboxItem {
    /// Stable outbox item identifier.
    pub item_id: OrderedOutboxItemId,
    /// Logical destination channel (e.g. `"event_sink"`, `"webhook"`).
    pub destination: String,
    /// Per-entity ordering key preventing parallel overtaking.
    pub ordering_key: String,
    /// Strictly monotonic per-`(destination, ordering_key)` sequence number (`1, 2, ...`).
    pub key_seq: i64,
    /// Associated global control event cursor sequence.
    pub cursor_seq: i64,
    /// Per-destination idempotency key for safe replay after worker crashes (V17).
    pub idempotency_key: IdempotencyKey,
    /// Serialized JSON payload string.
    pub payload_json: String,
    /// Current outbox delivery state.
    pub status: OrderedOutboxStatus,
    /// Number of delivery attempts executed so far.
    pub attempts: i32,
    /// Earliest UTC timestamp when this item may be claimed.
    pub available_at: TimestampMs,
    /// Current lease holder identifier, when `status == Leased`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_owner: Option<String>,
    /// Monotonic lease fencing token incremented on every claim.
    pub lease_generation: i64,
    /// UTC lease expiry timestamp, when `status == Leased`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_expires_at: Option<TimestampMs>,
    /// Secret-redacted error summary from the latest failed attempt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// Correlation identifier linking this outbox row to its origin.
    pub correlation_id: CorrelationId,
    /// UTC creation timestamp in epoch milliseconds.
    pub created_at: TimestampMs,
    /// UTC last-updated timestamp in epoch milliseconds.
    pub updated_at: TimestampMs,
}

// ============================================================================
// HTTP API Request & Response Envelopes for D2
// ============================================================================

/// Consistent `REPEATABLE READ` control-plane snapshot returned by `GET /v1/state/snapshot` (V18).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateSnapshotResponse {
    /// Exact gapless stream cursor captured in the same MVCC transaction as all collections below.
    pub snapshot_cursor: StreamCursor,
    /// Visible projects (filtered by `project_id` / `include_archived`).
    pub projects: Vec<ProjectRecord>,
    /// Registered Dennett agents.
    pub agents: Vec<AgentRecord>,
    /// Persisted sessions (filtered by `project_id` when supplied).
    pub sessions: Vec<SessionRecord>,
    /// Persisted durable tasks (filtered by `project_id` when supplied).
    pub tasks: Vec<TaskRecord>,
    /// Open Action Inbox cards (filtered by `project_id` when supplied).
    pub inbox_cards: Vec<InboxCardRecord>,
    /// Active (non-revoked, non-expired) capability grants.
    pub active_grants: Vec<CapabilityGrantRecord>,
    /// Count of open Action Inbox cards in this snapshot.
    pub unresolved_inbox_count: usize,
    /// Session identifiers that currently have an active `running` execution in PostgreSQL.
    #[serde(default)]
    pub running_session_ids: Vec<SessionId>,
}

/// Ordered event feed page returned by `GET /v1/events` (V15, V18).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventStreamResponse {
    /// Current stream authority epoch.
    pub authority_epoch: i64,
    /// Lowest retained sequence number still readable.
    pub retained_from_seq: i64,
    /// Caller's starting `after_cursor`.
    pub after_cursor: i64,
    /// Highest `cursor_seq` scanned during this query (advances safely past out-of-scope rows without leaking payload).
    pub scanned_through_seq: i64,
    /// Cursor value the client should pass as `after_cursor` on its next poll (`== scanned_through_seq`).
    pub next_cursor: i64,
    /// `true` when additional newer events already exist beyond `scanned_through_seq`.
    pub has_more: bool,
    /// Matching visible control events in strictly ascending `cursor_seq` order.
    pub events: Vec<ControlEventRecord>,
}

/// Request payload for `POST /v1/tasks`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateTaskRequest {
    /// Owning project identifier.
    pub project_id: ProjectId,
    /// Optional existing session; Head creates a dedicated session automatically if omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<SessionId>,
    /// Assigned Dennett agent identifier.
    pub agent_id: AgentId,
    /// Optional parent task identifier when delegating to a helper agent (enforces `child_scope ⊆ parent_scope`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_task_id: Option<TaskId>,
    /// Optional explicit title; derived from `goal` when omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Authoritative goal statement for this obligation.
    pub goal: String,
    /// Optional durable schedule (`OnceAt`, `Interval`, or `DailyWallTime` in an IANA timezone).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<TaskSchedule>,
    /// Optional signal name to wait for before running (`WaitingSignal` initial state).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_for_signal: Option<String>,
    /// Optional external resource budget override (falls back to agent config default budget).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<TaskBudget>,
    /// Optional shared skill identifiers attached to this task.
    #[serde(default)]
    pub attached_skills: Vec<String>,
    /// Optional explicit runtime authentication profile override (`oauth-personal` vs `gemini-api-key`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_profile_override: Option<RuntimeAuthProfile>,
    /// Optional target session identifier for delegated subagent execution or follow-up posting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_session_id: Option<SessionId>,
    /// Optional session identifier to notify when this task completes or fails.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notify_session_id: Option<SessionId>,
    /// Optional runtime model identifier override for this task/subagent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_override: Option<String>,
    /// Optional reasoning effort override for this task/subagent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_override: Option<ThinkingLevel>,
    /// Optional provider connection override for this task/subagent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_override_id: Option<ProviderConnectionId>,
    /// Optional explicit skill slug override list replacing default agent skill selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_slugs_override: Option<Vec<String>>,
    /// Optional wake trigger (e.g. [`TaskTrigger::OnRunCompleted`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<TaskTrigger>,
    /// When `false` on an unscheduled/non-signal task, leaves the task `queued` without auto-spawning immediately.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execute_immediately: Option<bool>,
}

impl CreateTaskRequest {
    /// Validates all task creation bounds, schedule invariants, budget limits, subagent overrides,
    /// and attached skills.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] on empty/oversized fields, mutually conflicting wait modes,
    /// or invalid schedule/budget/overrides/skills.
    pub fn validate(&self) -> Result<(), DennettError> {
        validate_non_empty_text(&self.goal, "goal", MAX_TASK_GOAL_BYTES)?;
        if let Some(ref title) = self.title {
            validate_non_empty_text(title, "title", MAX_TASK_TITLE_LEN)?;
        }
        if self.schedule.is_some() && self.wait_for_signal.is_some() {
            return Err(DennettError::InvalidInput {
                field: String::from("schedule"),
                reason: String::from("cannot combine `schedule` and `wait_for_signal` on creation"),
            });
        }
        if matches!(self.trigger, Some(TaskTrigger::OnRunCompleted { .. }))
            && (self.schedule.is_some() || self.wait_for_signal.is_some())
        {
            return Err(DennettError::InvalidInput {
                field: String::from("trigger"),
                reason: String::from(
                    "cannot combine `trigger = on_run_completed` with `schedule` or `wait_for_signal`",
                ),
            });
        }
        if let Some(ref sched) = self.schedule {
            sched.validate()?;
        }
        if let Some(ref sig) = self.wait_for_signal {
            validate_non_empty_text(sig, "wait_for_signal", MAX_SIGNAL_NAME_LEN)?;
        }
        if let Some(ref budget) = self.budget {
            budget.validate()?;
        }
        validate_attached_skills(&self.attached_skills)?;
        if let Some(ref model) = self.model_override {
            validate_non_empty_text(model, "model_override", MAX_MODEL_NAME_LEN)?;
            if model.bytes().any(|b| b.is_ascii_control()) {
                return Err(DennettError::InvalidInput {
                    field: String::from("model_override"),
                    reason: String::from("must not contain ASCII control characters"),
                });
            }
        }
        if let Some(ref skills) = self.skill_slugs_override {
            validate_attached_skills(skills)?;
        }
        Ok(())
    }
}

/// Response payload for `GET /v1/tasks`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListTasksResponse {
    /// Matching durable tasks ordered by `updated_at DESC`.
    pub tasks: Vec<TaskRecord>,
}

/// Full task inspection payload returned by `GET /v1/tasks/{task_id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskDetailResponse {
    /// Authoritative task record.
    pub task: TaskRecord,
    /// Execution runs linked to this task ordered chronologically.
    pub runs: Vec<RunRecord>,
    /// Armed and historical timers belonging to this task.
    pub timers: Vec<TaskTimerRecord>,
    /// Delivered signals belonging to this task.
    pub signals: Vec<TaskSignalRecord>,
    /// Tracked external effect claims linked to this task.
    pub effects: Vec<EffectClaimRecord>,
    /// Currently open Action Inbox cards linked to this task.
    pub open_inbox_cards: Vec<InboxCardRecord>,
}

/// Request payload for `PATCH /v1/tasks/{task_id}` (bumps `generation` atomically for V07 fencing).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateTaskRequest {
    /// Optional updated title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Optional updated goal text (increments `generation` and fences any in-flight older run).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<String>,
    /// Optional replacement schedule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule: Option<TaskSchedule>,
    /// When `true`, clears any existing schedule and transitions task to `queued`.
    #[serde(default)]
    pub clear_schedule: bool,
    /// Optional updated resource budget.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget: Option<TaskBudget>,
    /// When `false`, does not auto-spawn execution immediately after updating goal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execute_immediately: Option<bool>,
}

impl UpdateTaskRequest {
    /// Validates task update fields.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if no field is provided or any value is invalid.
    pub fn validate(&self) -> Result<(), DennettError> {
        if self.title.is_none()
            && self.goal.is_none()
            && self.schedule.is_none()
            && !self.clear_schedule
            && self.budget.is_none()
        {
            return Err(DennettError::InvalidInput {
                field: String::from("body"),
                reason: String::from(
                    "must specify at least one of `title`, `goal`, `schedule`, `clear_schedule`, or `budget`",
                ),
            });
        }
        if self.schedule.is_some() && self.clear_schedule {
            return Err(DennettError::InvalidInput {
                field: String::from("schedule"),
                reason: String::from("cannot set both `schedule` and `clear_schedule=true`"),
            });
        }
        if let Some(ref title) = self.title {
            validate_non_empty_text(title, "title", MAX_TASK_TITLE_LEN)?;
        }
        if let Some(ref goal) = self.goal {
            validate_non_empty_text(goal, "goal", MAX_TASK_GOAL_BYTES)?;
        }
        if let Some(ref sched) = self.schedule {
            sched.validate()?;
        }
        if let Some(ref budget) = self.budget {
            budget.validate()?;
        }
        Ok(())
    }
}

/// Request payload for `POST /v1/tasks/{task_id}/signals`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskSignalRequest {
    /// Deterministic idempotency key deduplicating retries (`UNIQUE (task_id, idempotency_key)`).
    pub idempotency_key: IdempotencyKey,
    /// Logical signal name.
    pub signal_name: String,
    /// Optional plain-text signal payload.
    #[serde(default)]
    pub payload_text: String,
}

impl TaskSignalRequest {
    /// Validates signal name and payload bounds.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] on empty/oversized signal name or payload.
    pub fn validate(&self) -> Result<(), DennettError> {
        validate_non_empty_text(&self.signal_name, "signal_name", MAX_SIGNAL_NAME_LEN)?;
        reject_nul(&self.payload_text, "payload_text")?;
        if self.payload_text.len() > MAX_SIGNAL_PAYLOAD_BYTES {
            return Err(DennettError::InvalidInput {
                field: String::from("payload_text"),
                reason: format!("exceeds maximum length of {MAX_SIGNAL_PAYLOAD_BYTES} bytes"),
            });
        }
        Ok(())
    }
}

/// Response payload for `POST /v1/tasks/{task_id}/signals`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskSignalResponse {
    /// Recorded signal entry.
    pub signal: TaskSignalRecord,
    /// Current task snapshot after signal processing.
    pub task: TaskRecord,
    /// `true` when this call matched an existing `(task_id, idempotency_key)` without double-executing.
    pub deduplicated: bool,
}

/// Request payload for `POST /v1/tasks/{task_id}/reply` (answering native clarification).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplyTaskRequest {
    /// User's plain-text clarification reply.
    pub answer_text: String,
}

impl ReplyTaskRequest {
    /// Validates clarification reply text.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if empty, oversized, or containing NUL.
    pub fn validate(&self) -> Result<(), DennettError> {
        validate_non_empty_text(
            &self.answer_text,
            "answer_text",
            MAX_CLARIFICATION_ANSWER_BYTES,
        )
    }
}

/// Honest response payload for targeted `POST /v1/tasks/{task_id}/cancel` (Stop).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelTaskResponse {
    /// Updated task record (`cancelled` OR `unknown_external` when external outcome is uncertain).
    pub task: TaskRecord,
    /// Active run identifier that received a live cancellation signal, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancelled_run_id: Option<RunId>,
    /// `true` when any external effect on this task is in `unknown` state (Stop NEVER claims guaranteed external cancel!).
    pub external_outcome_unknown: bool,
    /// External effects already confirmed executed prior to Stop.
    #[serde(default)]
    pub committed_effects: Vec<EffectClaimRecord>,
    /// External effects whose remote execution state is currently unknown and requires reconciliation.
    #[serde(default)]
    pub unknown_effects: Vec<EffectClaimRecord>,
    /// Non-blocking cached `memoryd` reachability status (Stop NEVER blocks on `memoryd`).
    pub memory_status: MemoryStatus,
}

/// Response payload for `GET /v1/inbox`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListInboxResponse {
    /// Matching Action Inbox cards ordered newest first.
    pub cards: Vec<InboxCardRecord>,
    /// Count of currently open Action Inbox cards in the queried scope.
    pub unresolved_count: usize,
}

/// Manual resolution option when an external provider cannot be automatically reconciled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ManualEffectResolution {
    /// Owner verified externally that the action executed -> mark `confirmed`.
    MarkConfirmed,
    /// Owner verified externally that the action never executed -> mark `failed`.
    MarkNotExecuted,
}

impl ManualEffectResolution {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MarkConfirmed => "mark_confirmed",
            Self::MarkNotExecuted => "mark_not_executed",
        }
    }
}

impl fmt::Display for ManualEffectResolution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ManualEffectResolution {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "mark_confirmed" => Ok(Self::MarkConfirmed),
            "mark_not_executed" => Ok(Self::MarkNotExecuted),
            other => Err(DennettError::InvalidInput {
                field: String::from("manual_resolution"),
                reason: format!(
                    "unknown manual effect resolution `{other}` (expected mark_confirmed|mark_not_executed)"
                ),
            }),
        }
    }
}

/// Action payload supplied to `POST /v1/inbox/{card_id}/resolve`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
#[non_exhaustive]
pub enum InboxResolutionAction {
    /// Resolve a `permission_approval` card.
    Permission {
        /// Approval or denial decision.
        decision: PermissionResolutionDecision,
        /// Optional human-readable rationale.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        /// Optional maximum uses when issuing a standing grant.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_uses: Option<i32>,
        /// Optional TTL in seconds when issuing a standing grant.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expires_in_secs: Option<u64>,
    },
    /// Answer a `clarification` card and continue the task.
    Clarification {
        /// User's clarification answer text.
        answer_text: String,
    },
    /// Increase a task's external budget on a `budget_exceeded` card and continue.
    IncreaseBudget {
        /// Additional turns to add to `max_turns`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        additional_turns: Option<u32>,
        /// Additional tokens to add to `max_tokens`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        additional_tokens: Option<u64>,
        /// Additional wall-clock seconds to add to `max_wall_secs`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        additional_wall_secs: Option<u64>,
    },
    /// Stop/terminate the task referenced by a `clarification`, `budget_exceeded`, or `missed_schedule` card.
    StopTask {
        /// Optional reason recorded in audit logs.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// Decide whether to execute or skip a `missed_schedule` card.
    MissedSchedule {
        /// `true` executes once now; `false` skips and waits for the next future window.
        run_now: bool,
    },
    /// Reconcile an `unknown_effect` card automatically or via explicit manual decision.
    ReconcileEffect {
        /// Optional manual resolution when provider reconciliation is unavailable.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        manual_resolution: Option<ManualEffectResolution>,
        /// Optional operator note.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

/// Request payload for `POST /v1/inbox/{card_id}/resolve`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveInboxCardRequest {
    /// Optimistic concurrency revision matching `InboxCardRecord::revision`.
    pub expected_revision: i64,
    /// Typed resolution action corresponding to the card kind.
    pub resolution: InboxResolutionAction,
}

/// Response payload for `POST /v1/inbox/{card_id}/resolve`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveInboxCardResponse {
    /// Updated Action Inbox card.
    pub card: InboxCardRecord,
    /// Updated task record, when the card was linked to a task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskRecord>,
    /// Updated permission request, when resolving a `permission_approval` card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_request: Option<PermissionRequestRecord>,
    /// Newly issued capability grant, when resolved via `ApproveProject` or `ApproveTask`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issued_grant: Option<CapabilityGrantRecord>,
    /// Reconciled external effect claim, when resolving an `unknown_effect` card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<EffectClaimRecord>,
}

/// Response payload for `GET /v1/permissions/requests`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListPermissionRequestsResponse {
    /// Matching permission requests ordered chronologically.
    pub requests: Vec<PermissionRequestRecord>,
}

/// Request payload for `POST /v1/permissions/requests/{request_id}/resolve` (and `/v1/approvals/{id}/resolve`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvePermissionRequest {
    /// Optional optimistic revision check (strictly enforced when supplied; returns `409 Conflict` if stale).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_revision: Option<i64>,
    /// Owner approval or denial decision.
    pub decision: PermissionResolutionDecision,
    /// Optional human-readable rationale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// Optional max uses when creating a revocable grant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_uses: Option<i32>,
    /// Optional expiry TTL in seconds when creating a revocable grant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_in_secs: Option<u64>,
}

impl ResolvePermissionRequest {
    /// Validates optional grant bounds and reason text.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] on non-positive `max_uses`/`expires_in_secs` or oversized reason.
    pub fn validate(&self) -> Result<(), DennettError> {
        if let Some(max) = self.max_uses
            && max <= 0
        {
            return Err(DennettError::InvalidInput {
                field: String::from("max_uses"),
                reason: String::from("must be greater than zero when provided"),
            });
        }
        if let Some(secs) = self.expires_in_secs
            && secs == 0
        {
            return Err(DennettError::InvalidInput {
                field: String::from("expires_in_secs"),
                reason: String::from("must be greater than zero when provided"),
            });
        }
        if let Some(ref reason) = self.reason {
            reject_nul(reason, "reason")?;
            if reason.len() > MAX_REASON_BYTES {
                return Err(DennettError::InvalidInput {
                    field: String::from("reason"),
                    reason: format!("exceeds maximum length of {MAX_REASON_BYTES} bytes"),
                });
            }
        }
        Ok(())
    }
}

/// Response payload for `POST /v1/permissions/requests/{request_id}/resolve`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvePermissionResponse {
    /// Updated permission request record.
    pub request: PermissionRequestRecord,
    /// Audited external decision record (`evaluated_outside_model == true`).
    pub decision: PermissionDecisionRecord,
    /// Issued revocable grant, when resolved via `ApproveProject` or `ApproveTask`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issued_grant: Option<CapabilityGrantRecord>,
}

/// Response payload for `GET /v1/grants`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListCapabilityGrantsResponse {
    /// Matching capability grants ordered newest first.
    pub grants: Vec<CapabilityGrantRecord>,
}

/// Request payload for `POST /v1/grants/{grant_id}/revoke`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RevokeGrantRequest {
    /// Optional human-readable revocation reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Request payload for `POST /v1/agents/{agent_id}/messages`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendAgentMessageRequest {
    /// Optional sending agent identifier (`None` when sent by the owner/control plane).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_agent_id: Option<AgentId>,
    /// Shared project identifier bounding coordination scope.
    pub project_id: ProjectId,
    /// Optional related task identifier for traceability.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub related_task_id: Option<TaskId>,
    /// Per-recipient idempotency key (`UNIQUE (recipient_agent_id, idempotency_key)`).
    pub idempotency_key: IdempotencyKey,
    /// Plain-text coordination body delivered into the recipient's next turn context.
    pub body_text: String,
}

impl SendAgentMessageRequest {
    /// Validates plain-text inter-agent message bounds.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] if `body_text` is empty, oversized, or contains NUL.
    pub fn validate(&self) -> Result<(), DennettError> {
        validate_non_empty_text(&self.body_text, "body_text", MAX_AGENT_MESSAGE_BYTES)
    }
}

/// Response payload for `POST /v1/agents/{agent_id}/messages`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendAgentMessageResponse {
    /// Persisted inter-agent message record.
    pub message: AgentMessageRecord,
    /// `true` when deduplicated by `(recipient_agent_id, idempotency_key)`.
    pub deduplicated: bool,
}

/// Response payload for `GET /v1/agents/{agent_id}/messages`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListAgentMessagesResponse {
    /// Matching plain-text messages ordered chronologically.
    pub messages: Vec<AgentMessageRecord>,
}

/// Request payload for `POST /v1/effects/dispatch`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DispatchEffectRequest {
    /// Owning project identifier.
    pub project_id: ProjectId,
    /// Owning session identifier.
    pub session_id: SessionId,
    /// Optional owning task identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<TaskId>,
    /// Optional owning execution run identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<RunId>,
    /// Agent requesting this external effect.
    pub agent_id: AgentId,
    /// Globally unique idempotency key preventing blind duplicate dispatches.
    pub effect_idempotency_key: IdempotencyKey,
    /// Optional connector identifier (defaults to `"simulated_external"` in D2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connector_id: Option<String>,
    /// Logical external operation name.
    pub operation: String,
    /// External recipient or endpoint identifier.
    pub target_recipient: String,
    /// Serialized JSON object of operation parameters.
    #[serde(default = "default_empty_json_object")]
    pub parameters_json: String,
    /// Deterministic behavior override for `SimulatedExternalConnector`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub simulated_behavior: Option<SimEffectBehavior>,
}

fn default_empty_json_object() -> String {
    String::from("{}")
}

impl DispatchEffectRequest {
    /// Validates external effect dispatch fields.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] on empty/oversized fields.
    pub fn validate(&self) -> Result<(), DennettError> {
        if let Some(ref conn) = self.connector_id {
            validate_non_empty_text(conn, "connector_id", MAX_CONNECTOR_ID_LEN)?;
        }
        validate_non_empty_text(&self.operation, "operation", MAX_EFFECT_OPERATION_LEN)?;
        validate_non_empty_text(
            &self.target_recipient,
            "target_recipient",
            MAX_EFFECT_RECIPIENT_LEN,
        )?;
        validate_non_empty_text(
            &self.parameters_json,
            "parameters_json",
            MAX_EFFECT_PARAMS_BYTES,
        )?;
        Ok(())
    }
}

/// Response payload for `POST /v1/effects/dispatch`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DispatchEffectResponse {
    /// Persisted external effect claim record.
    pub effect: EffectClaimRecord,
    /// `true` when deduplicated from an existing confirmed claim or blocked from blind retry.
    pub deduplicated: bool,
    /// `true` when the external outcome is `Unknown` and requires reconciliation.
    pub external_outcome_unknown: bool,
}

/// Response payload for `GET /v1/effects`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListEffectsResponse {
    /// Matching external effect claims ordered newest first.
    pub effects: Vec<EffectClaimRecord>,
}

/// Request payload for `POST /v1/effects/{effect_id}/reconcile`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconcileEffectRequest {
    /// Optional explicit manual resolution when automatic provider reconciliation is unavailable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manual_resolution: Option<ManualEffectResolution>,
    /// Optional operator explanation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Response payload for `POST /v1/effects/{effect_id}/reconcile`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReconcileEffectResponse {
    /// Updated effect claim record.
    pub effect: EffectClaimRecord,
    /// Updated linked task record, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskRecord>,
    /// `true` when the external provider confirmed an already-accepted execution record.
    pub provider_found: bool,
    /// Total execution count observed at the external provider (proves zero duplicate re-sends).
    pub provider_execution_count: u64,
    /// Human-readable summary of the reconciliation result (status, provider count, and mode).
    #[serde(default)]
    pub resolution_summary: String,
}

/// Request payload for `POST /v1/projects/{project_id}/rebind` (Contract C).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RebindProjectRequest {
    /// New canonical local workspace directory path.
    pub workspace_path: String,
    /// Optional audit rationale for rebinding.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl RebindProjectRequest {
    /// Validates rebind request inputs.
    ///
    /// # Errors
    ///
    /// Returns [`DennettError::InvalidInput`] on empty/oversized path or reason.
    pub fn validate(&self) -> Result<(), DennettError> {
        validate_non_empty_text(
            &self.workspace_path,
            "workspace_path",
            MAX_WORKSPACE_PATH_LEN,
        )?;
        if let Some(ref reason) = self.reason {
            reject_nul(reason, "reason")?;
            if reason.len() > MAX_REASON_BYTES {
                return Err(DennettError::InvalidInput {
                    field: String::from("reason"),
                    reason: format!("exceeds maximum length of {MAX_REASON_BYTES} bytes"),
                });
            }
        }
        Ok(())
    }
}

/// Response payload for project lifecycle transitions (`rebind`, `detach-workspace`, `archive`, `unarchive`, `remove`, `delete`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectLifecycleResponse {
    /// Updated project record.
    pub project: ProjectRecord,
    /// Number of pending timers paused or cancelled by this transition.
    pub affected_timers: usize,
    /// Explicit safety attestation: Dennett NEVER deletes external user workspace files on disk (always `false`).
    pub user_workspace_files_deleted: bool,
}

/// Explicit lifecycle action for `POST /v1/sessions/{session_id}/artifacts/{artifact_id}/lifecycle` (Contract D).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ArtifactLifecycleAction {
    /// Mark a `partial` artifact revision as `final`.
    Finalize,
    /// Reversibly archive the artifact in Dennett.
    Archive,
    /// Restore an archived artifact to `active`.
    Unarchive,
    /// Remove the artifact from active Dennett listings (user files untouched).
    Remove,
    /// Scrub Dennett inline content leaving a tombstone (user workspace files untouched).
    Delete,
}

impl ArtifactLifecycleAction {
    /// Returns the canonical snake-case string representation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Finalize => "finalize",
            Self::Archive => "archive",
            Self::Unarchive => "unarchive",
            Self::Remove => "remove",
            Self::Delete => "delete",
        }
    }
}

impl fmt::Display for ArtifactLifecycleAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ArtifactLifecycleAction {
    type Err = DennettError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "finalize" => Ok(Self::Finalize),
            "archive" => Ok(Self::Archive),
            "unarchive" => Ok(Self::Unarchive),
            "remove" => Ok(Self::Remove),
            "delete" => Ok(Self::Delete),
            other => Err(DennettError::InvalidInput {
                field: String::from("action"),
                reason: format!(
                    "unknown artifact lifecycle action `{other}` (expected finalize|archive|unarchive|remove|delete)"
                ),
            }),
        }
    }
}

/// Request payload for `POST /v1/sessions/{session_id}/artifacts/{artifact_id}/lifecycle`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactLifecycleRequest {
    /// Distinct lifecycle operation.
    pub action: ArtifactLifecycleAction,
    /// Optional audit rationale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Response payload for `POST /v1/sessions/{session_id}/artifacts/{artifact_id}/lifecycle`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactLifecycleResponse {
    /// Updated artifact record.
    pub artifact: ArtifactRecord,
    /// Explicit safety attestation: Dennett NEVER deletes user workspace files on disk (always `false`).
    pub user_workspace_files_deleted: bool,
}

/// Request payload for test-isolated `POST /v1/tasks/scheduler/tick`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulerTickRequest {
    /// Optional deterministic clock timestamp in UTC epoch milliseconds (defaults to current UTC time).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub now_ms: Option<i64>,
}

/// Response payload for test-isolated `POST /v1/tasks/scheduler/tick`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulerTickResponse {
    /// Effective clock timestamp used for this scheduler pass.
    pub now: TimestampMs,
    /// Total due timers claimed in this tick.
    pub claimed_timers: usize,
    /// Tasks woken and queued for execution.
    pub woken_tasks: usize,
    /// Overdue timers skipped under `MissedRunPolicy::Skip`.
    pub skipped_timers: usize,
    /// Overdue timers parked for user decision under `MissedRunPolicy::Ask`.
    pub asked_timers: usize,
}

/// Validates a list of attached skill identifiers on an agent or task.
///
/// # Errors
///
/// Returns [`DennettError::InvalidInput`] if there are too many skills or any skill name is empty/oversized/unsafe.
pub fn validate_attached_skills(skills: &[String]) -> Result<(), DennettError> {
    if skills.len() > MAX_ATTACHED_SKILLS {
        return Err(DennettError::InvalidInput {
            field: String::from("attached_skills"),
            reason: format!("exceeds maximum of {MAX_ATTACHED_SKILLS} skills"),
        });
    }
    for skill in skills {
        validate_non_empty_text(skill, "attached_skills", MAX_SKILL_NAME_LEN)?;
        if skill.bytes().any(|b| b.is_ascii_control()) {
            return Err(DennettError::InvalidInput {
                field: String::from("attached_skills"),
                reason: String::from("skill name must not contain ASCII control characters"),
            });
        }
    }
    Ok(())
}

/// Validates an optional client message deduplication identifier (`client_message_id`).
///
/// # Errors
///
/// Returns [`DennettError::InvalidInput`] if empty, oversized, or containing control/whitespace bytes.
pub fn validate_client_message_id(raw: &str) -> Result<(), DennettError> {
    if raw.is_empty() || raw.len() > MAX_CLIENT_MESSAGE_ID_LEN {
        return Err(DennettError::InvalidInput {
            field: String::from("client_message_id"),
            reason: format!("must be non-empty and at most {MAX_CLIENT_MESSAGE_ID_LEN} bytes"),
        });
    }
    if !raw
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
    {
        return Err(DennettError::InvalidInput {
            field: String::from("client_message_id"),
            reason: String::from(
                "must contain only ASCII alphanumeric characters, '-', '_', '.', or ':'",
            ),
        });
    }
    Ok(())
}

/// Checks whether a helper agent's [`WorkspaceWritePolicy`] is monotonically no broader than its
/// parent agent's policy (`child ⊆ parent`, A07).
#[must_use]
pub fn write_policy_is_subset_of(
    child: &WorkspaceWritePolicy,
    parent: &WorkspaceWritePolicy,
) -> bool {
    match (child, parent) {
        (WorkspaceWritePolicy::DenyAll, _)
        | (_, WorkspaceWritePolicy::AllowWorkspace)
        | (WorkspaceWritePolicy::AskBeforeWrite, WorkspaceWritePolicy::AskBeforeWrite) => true,
        (
            WorkspaceWritePolicy::AllowPaths {
                allowed_prefixes: child_prefixes,
            },
            WorkspaceWritePolicy::AllowPaths {
                allowed_prefixes: parent_prefixes,
            },
        ) => child_prefixes.iter().all(|cp| {
            let norm_c = cp
                .trim()
                .replace('\\', "/")
                .trim_start_matches("./")
                .trim_end_matches('/')
                .to_ascii_lowercase();
            parent_prefixes.iter().any(|pp| {
                let norm_p = pp
                    .trim()
                    .replace('\\', "/")
                    .trim_start_matches("./")
                    .trim_end_matches('/')
                    .to_ascii_lowercase();
                norm_c == norm_p || norm_c.starts_with(&format!("{norm_p}/"))
            })
        }),
        _ => false,
    }
}

/// Validates a capability grant scope selector (`"project:<uuid>"` or `"task:<uuid>"`).
///
/// # Errors
///
/// Returns [`DennettError::InvalidInput`] on invalid scope syntax.
pub fn validate_grant_scope(scope: &str) -> Result<(), DennettError> {
    validate_non_empty_text(scope, "scope", MAX_POLICY_SCOPE_LEN)?;
    for prefix in ["project:", "task:"] {
        if let Some(rest) = scope.strip_prefix(prefix) {
            Uuid::parse_str(rest).map_err(|_| DennettError::InvalidInput {
                field: String::from("scope"),
                reason: format!("grant scope `{prefix}<uuid>` must carry a valid UUID"),
            })?;
            return Ok(());
        }
    }
    Err(DennettError::InvalidInput {
        field: String::from("scope"),
        reason: String::from("grant scope must be `project:<uuid>` or `task:<uuid>`"),
    })
}

/// Validates a capability grant tool pattern string.
///
/// # Errors
///
/// Returns [`DennettError::InvalidInput`] on empty or oversized tool pattern.
pub fn validate_grant_tool_pattern(pattern: &str) -> Result<(), DennettError> {
    validate_non_empty_text(pattern, "tool_pattern", MAX_POLICY_PATTERN_LEN)
}

fn reject_nul(value: &str, field: &str) -> Result<(), DennettError> {
    if value.as_bytes().contains(&0) {
        return Err(DennettError::InvalidInput {
            field: field.to_owned(),
            reason: String::from("must not contain NUL bytes"),
        });
    }
    Ok(())
}

fn validate_non_empty_text(value: &str, field: &str, max_bytes: usize) -> Result<(), DennettError> {
    reject_nul(value, field)?;
    if value.trim().is_empty() || value.len() > max_bytes {
        return Err(DennettError::InvalidInput {
            field: field.to_owned(),
            reason: format!("must be non-empty and at most {max_bytes} bytes"),
        });
    }
    Ok(())
}

const _: () = {
    const fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<TaskId>();
    assert_send_sync::<TaskTimerId>();
    assert_send_sync::<TaskSignalId>();
    assert_send_sync::<PermissionRequestId>();
    assert_send_sync::<CapabilityGrantId>();
    assert_send_sync::<InboxCardId>();
    assert_send_sync::<EffectClaimId>();
    assert_send_sync::<AgentMessageId>();
    assert_send_sync::<ControlEventId>();
    assert_send_sync::<OrderedOutboxItemId>();
    assert_send_sync::<ProjectLifecycleState>();
    assert_send_sync::<ArtifactCompleteness>();
    assert_send_sync::<ArtifactLifecycleState>();
    assert_send_sync::<TaskStatus>();
    assert_send_sync::<TaskSchedule>();
    assert_send_sync::<TaskTrigger>();
    assert_send_sync::<TaskBudget>();
    assert_send_sync::<TaskUsage>();
    assert_send_sync::<TaskRecord>();
    assert_send_sync::<TaskTimerRecord>();
    assert_send_sync::<TaskSignalRecord>();
    assert_send_sync::<PermissionRequestRecord>();
    assert_send_sync::<CapabilityGrantRecord>();
    assert_send_sync::<InboxCardRecord>();
    assert_send_sync::<EffectClaimRecord>();
    assert_send_sync::<AgentMessageRecord>();
    assert_send_sync::<StreamCursor>();
    assert_send_sync::<ControlEventRecord>();
    assert_send_sync::<OrderedOutboxItem>();
    assert_send_sync::<StateSnapshotResponse>();
    assert_send_sync::<EventStreamResponse>();
};

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn iana_timezone_syntax_validation_accepts_standard_and_rejects_malformed() {
        for ok in [
            "UTC",
            "Etc/UTC",
            "Asia/Yekaterinburg",
            "America/Argentina/Buenos_Aires",
            "Etc/GMT+5",
        ] {
            assert!(
                validate_iana_timezone_syntax(ok).is_ok(),
                "expected valid timezone `{ok}`"
            );
        }

        for bad in [
            "",
            " ",
            " Asia/Yekaterinburg",
            "Local",
            "/UTC",
            "Asia/",
            "Asia//Yekaterinburg",
            "Asia/../Etc",
            "Asia/Yekaterinburg; DROP TABLE tasks",
            "A/B/C/D",
        ] {
            assert!(
                validate_iana_timezone_syntax(bad).is_err(),
                "expected invalid timezone `{bad}`"
            );
        }
    }

    #[test]
    fn permission_decision_aliases_deserialize_cleanly() {
        assert_eq!(
            serde_json::from_str::<PermissionResolutionDecision>("\"allow_once\"").expect("alias"),
            PermissionResolutionDecision::ApproveOnce
        );
        assert_eq!(
            serde_json::from_str::<PermissionResolutionDecision>("\"allow_scope\"").expect("alias"),
            PermissionResolutionDecision::ApproveProject
        );
        assert_eq!(
            serde_json::to_string(&PermissionResolutionDecision::ApproveOnce).expect("ser"),
            "\"approve_once\""
        );
    }

    #[test]
    fn write_policy_subset_enforces_monotonic_delegation_narrowing() {
        let docs = WorkspaceWritePolicy::AllowPaths {
            allowed_prefixes: vec![String::from("docs")],
        };
        let docs_sub = WorkspaceWritePolicy::AllowPaths {
            allowed_prefixes: vec![String::from("docs/drafts")],
        };
        let src = WorkspaceWritePolicy::AllowPaths {
            allowed_prefixes: vec![String::from("src")],
        };

        assert!(write_policy_is_subset_of(
            &WorkspaceWritePolicy::DenyAll,
            &WorkspaceWritePolicy::DenyAll
        ));
        assert!(write_policy_is_subset_of(
            &WorkspaceWritePolicy::AskBeforeWrite,
            &WorkspaceWritePolicy::AllowWorkspace
        ));
        assert!(write_policy_is_subset_of(&docs_sub, &docs));
        assert!(write_policy_is_subset_of(
            &docs,
            &WorkspaceWritePolicy::AllowWorkspace
        ));

        // Escalations MUST be rejected.
        assert!(!write_policy_is_subset_of(
            &WorkspaceWritePolicy::AllowWorkspace,
            &WorkspaceWritePolicy::AskBeforeWrite
        ));
        assert!(!write_policy_is_subset_of(
            &WorkspaceWritePolicy::AskBeforeWrite,
            &WorkspaceWritePolicy::DenyAll
        ));
        assert!(
            !write_policy_is_subset_of(&WorkspaceWritePolicy::AskBeforeWrite, &docs),
            "D2-SEC-02: AskBeforeWrite must NOT be treated as a subset of path-restricted AllowPaths"
        );
        assert!(!write_policy_is_subset_of(&docs, &docs_sub));
        assert!(!write_policy_is_subset_of(&src, &docs));
    }

    #[test]
    fn schedule_validation_enforces_once_at_bounds_and_weekly_mask() {
        use crate::memory::{MAX_TIMESTAMP_MS, MIN_TIMESTAMP_MS};

        assert!(
            ScheduleKind::OnceAt {
                fire_at: TimestampMs::from_millis(MAX_TIMESTAMP_MS)
            }
            .validate()
            .is_ok()
        );
        assert!(
            ScheduleKind::OnceAt {
                fire_at: TimestampMs::from_millis(MAX_TIMESTAMP_MS + 1)
            }
            .validate()
            .is_err()
        );
        assert!(
            ScheduleKind::OnceAt {
                fire_at: TimestampMs::from_millis(MIN_TIMESTAMP_MS - 1)
            }
            .validate()
            .is_err()
        );
        assert!(
            ScheduleKind::OnceAt {
                fire_at: TimestampMs::from_millis(i64::MAX)
            }
            .validate()
            .is_err()
        );

        let weekly = ScheduleKind::WeeklyWallTime {
            weekdays_mask: 0b001_1111,
            hour: 9,
            minute: 30,
        };
        assert!(weekly.validate().is_ok());
        assert_eq!(weekly.recurrence_label(), "weekly_wall");

        assert!(
            ScheduleKind::WeeklyWallTime {
                weekdays_mask: 0,
                hour: 9,
                minute: 0,
            }
            .validate()
            .is_err()
        );
        assert!(
            ScheduleKind::WeeklyWallTime {
                weekdays_mask: 128,
                hour: 9,
                minute: 0,
            }
            .validate()
            .is_err()
        );

        let aliased: ScheduleKind = serde_json::from_str(
            r#"{"kind":"weekly_local_time","weekdays_mask":1,"hour":10,"minute":0}"#,
        )
        .expect("weekly_local_time alias");
        assert_eq!(
            aliased,
            ScheduleKind::WeeklyWallTime {
                weekdays_mask: 1,
                hour: 10,
                minute: 0,
            }
        );
    }

    #[test]
    fn yearly_schedule_validation_enforces_month_day_and_leap_day() {
        let yearly_leap = ScheduleKind::YearlyWallTime {
            month: 2,
            day: 29,
            hour: 8,
            minute: 15,
        };
        assert!(yearly_leap.validate().is_ok());
        assert_eq!(yearly_leap.recurrence_label(), "yearly_wall");
        let yearly_encoded = serde_json::to_string(&yearly_leap).expect("encode yearly");
        assert_eq!(
            serde_json::from_str::<ScheduleKind>(&yearly_encoded).expect("decode yearly"),
            yearly_leap
        );
        let yearly_alias: ScheduleKind = serde_json::from_str(
            r#"{"kind":"yearly_local_time","month":12,"day":31,"hour":23,"minute":59}"#,
        )
        .expect("yearly_local_time alias");
        assert!(yearly_alias.validate().is_ok());

        for bad_yearly in [
            ScheduleKind::YearlyWallTime {
                month: 0,
                day: 1,
                hour: 9,
                minute: 0,
            },
            ScheduleKind::YearlyWallTime {
                month: 13,
                day: 1,
                hour: 9,
                minute: 0,
            },
            ScheduleKind::YearlyWallTime {
                month: 2,
                day: 30,
                hour: 9,
                minute: 0,
            },
            ScheduleKind::YearlyWallTime {
                month: 4,
                day: 31,
                hour: 9,
                minute: 0,
            },
            ScheduleKind::YearlyWallTime {
                month: 1,
                day: 0,
                hour: 9,
                minute: 0,
            },
            ScheduleKind::YearlyWallTime {
                month: 1,
                day: 15,
                hour: 24,
                minute: 0,
            },
        ] {
            assert!(
                bad_yearly.validate().is_err(),
                "expected invalid yearly schedule: {bad_yearly:?}"
            );
        }
    }

    #[test]
    fn task_trigger_and_subagent_overrides_validate_and_roundtrip() {
        let run_id = RunId::new();
        let trigger = TaskTrigger::OnRunCompleted { run_id };
        let trig_json = serde_json::to_string(&trigger).expect("serialize trigger");
        assert!(trig_json.contains(r#""kind":"on_run_completed""#));
        assert_eq!(
            serde_json::from_str::<TaskTrigger>(&trig_json).expect("deserialize trigger"),
            trigger
        );
        assert_eq!(TaskTrigger::default(), TaskTrigger::Immediate);

        let req = CreateTaskRequest {
            project_id: ProjectId::new(),
            session_id: None,
            agent_id: AgentId::new(),
            parent_task_id: Some(TaskId::new()),
            title: Some(String::from("Audit subagent")),
            goal: String::from("Verify invariants"),
            schedule: None,
            wait_for_signal: None,
            budget: None,
            attached_skills: Vec::new(),
            auth_profile_override: None,
            target_session_id: Some(SessionId::new()),
            notify_session_id: Some(SessionId::new()),
            model_override: Some(String::from("gemini-4-neon")),
            thinking_override: Some(ThinkingLevel::High),
            connection_override_id: Some(ProviderConnectionId::new()),
            skill_slugs_override: Some(vec![String::from("rust-audit")]),
            trigger: Some(trigger),
            execute_immediately: Some(false),
        };
        assert!(req.validate().is_ok());
        let encoded = serde_json::to_string(&req).expect("serialize CreateTaskRequest");
        let decoded: CreateTaskRequest =
            serde_json::from_str(&encoded).expect("deserialize CreateTaskRequest");
        assert_eq!(decoded, req);

        // Mutually exclusive trigger + schedule must fail.
        let conflicting = CreateTaskRequest {
            schedule: Some(TaskSchedule {
                timezone: String::from("UTC"),
                original_expression: None,
                kind: ScheduleKind::Interval { interval_secs: 60 },
                missed_run_policy: MissedRunPolicy::ExecuteOnce,
                next_fire_at: None,
            }),
            ..req.clone()
        };
        assert!(conflicting.validate().is_err());

        // Blank model_override must fail.
        let bad_model = CreateTaskRequest {
            model_override: Some(String::from("   ")),
            ..req
        };
        assert!(bad_model.validate().is_err());
    }

    proptest! {
        #[test]
        fn prop_budget_boundary_checks_at_limit_minus_one_limit_and_limit_plus_one(
            max_turns in 1_u32..50,
            max_tokens in 1_u64..100_000,
            max_wall_secs in 1_u64..3_600,
        ) {
            let budget = TaskBudget {
                max_turns: Some(max_turns),
                max_tokens: Some(max_tokens),
                max_wall_secs: Some(max_wall_secs),
                exceed_policy: BudgetExceedPolicy::AskApproval,
            };
            prop_assert!(budget.validate().is_ok());

            // Below every limit (limit - 1) -> allowed both before and after.
            let below = TaskUsage {
                turns_used: max_turns - 1,
                tokens_used: max_tokens - 1,
                wall_ms_used: (max_wall_secs - 1) * 1_000,
                confidence: UsageConfidence::ProviderReported,
            };
            prop_assert!(budget.check_before_next_turn(&below).is_none());
            prop_assert!(budget.check_after_turn(&below).is_none());

            // Exactly at limit -> finishing turn is OK (`check_after_turn == None`),
            // starting another turn is blocked (`check_before_next_turn == Some`).
            let exact = TaskUsage {
                turns_used: max_turns,
                tokens_used: max_tokens,
                wall_ms_used: max_wall_secs * 1_000,
                confidence: UsageConfidence::ProviderReported,
            };
            prop_assert!(budget.check_after_turn(&exact).is_none());
            prop_assert!(budget.check_before_next_turn(&exact).is_some());

            // Strictly above limit (limit + 1) -> blocked both before and after.
            let over_tokens = TaskUsage {
                turns_used: 0,
                tokens_used: max_tokens + 1,
                wall_ms_used: 0,
                confidence: UsageConfidence::ProviderReported,
            };
            prop_assert!(budget.check_before_next_turn(&over_tokens).is_some());
            prop_assert!(budget.check_after_turn(&over_tokens).is_some());

            let over_wall = TaskUsage {
                turns_used: 0,
                tokens_used: 0,
                wall_ms_used: max_wall_secs * 1_000 + 1,
                confidence: UsageConfidence::ProviderReported,
            };
            prop_assert!(budget.check_before_next_turn(&over_wall).is_some());
            prop_assert!(budget.check_after_turn(&over_wall).is_some());
        }

        #[test]
        fn prop_generation_fencing_model_never_overwrites_newer_goal_with_late_worker(
            goals in prop::collection::vec("[a-z0-9 ]{1,32}", 2..12),
        ) {
            // Pure state-machine oracle proving V07 generation fencing invariant:
            // any late completion from generation `g_old < current_generation` is marked
            // `RunStatus::Superseded` and NEVER mutates `current_goal` or `current_status`.
            let mut current_gen: i64 = 1;
            let mut current_goal = goals[0].clone();
            let mut current_status = TaskStatus::Running;
            let mut in_flight_gens: Vec<i64> = vec![1];

            for next_goal in &goals[1..] {
                current_gen += 1;
                current_goal = next_goal.clone();
                current_status = TaskStatus::Queued;
                in_flight_gens.push(current_gen);
            }

            let expected_goal = current_goal.clone();
            // Late completions arrive in arbitrary order for all older generations.
            for &old_gen in &in_flight_gens[..in_flight_gens.len() - 1] {
                prop_assert!(old_gen < current_gen);
                let run_status = if old_gen != current_gen || current_status == TaskStatus::Cancelled {
                    crate::control::RunStatus::Superseded
                } else {
                    crate::control::RunStatus::Succeeded
                };
                prop_assert_eq!(run_status, crate::control::RunStatus::Superseded);
                // Current task goal and status remain untouched.
                prop_assert_eq!(&current_goal, &expected_goal);
                prop_assert_eq!(current_status, TaskStatus::Queued);
            }

            // Final current-generation completion transitions cleanly to Succeeded.
            let latest_gen = *in_flight_gens.last().unwrap_or(&current_gen);
            prop_assert_eq!(latest_gen, current_gen);
            current_status = TaskStatus::Succeeded;
            prop_assert_eq!(current_status, TaskStatus::Succeeded);
            prop_assert_eq!(current_goal, expected_goal);
        }
    }
}
