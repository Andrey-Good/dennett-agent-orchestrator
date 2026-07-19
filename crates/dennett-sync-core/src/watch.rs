use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WatchCursor {
    pub stream_id: String,
    pub sequence: u64,
    pub authority_epoch: u64,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResyncReason {
    SequenceGap,
    RevisionGap,
    AuthorityEpochChanged,
    StreamReplaced,
    SnapshotInvalid,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WatchError {
    pub code: String,
    pub message_key: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CachedWatch<T> {
    pub cursor: WatchCursor,
    pub revision: u64,
    pub fingerprint: Vec<u8>,
    pub value: T,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WatchState<T> {
    Empty,
    Fresh(CachedWatch<T>),
    Stale {
        cached: CachedWatch<T>,
        reason: ResyncReason,
    },
    Resyncing {
        cached: Option<CachedWatch<T>>,
        reason: ResyncReason,
    },
    Unavailable {
        cached: Option<CachedWatch<T>>,
        error: WatchError,
    },
    Revoked,
}

impl<T: Clone> WatchState<T> {
    #[must_use]
    pub fn cached(&self) -> Option<CachedWatch<T>> {
        match self {
            Self::Fresh(cached) | Self::Stale { cached, .. } => Some(cached.clone()),
            Self::Resyncing { cached, .. } | Self::Unavailable { cached, .. } => cached.clone(),
            Self::Empty | Self::Revoked => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WatchFrame<T, D> {
    Snapshot {
        cursor: WatchCursor,
        revision: u64,
        fingerprint: Vec<u8>,
        value: T,
    },
    Delta {
        cursor: WatchCursor,
        base_revision: u64,
        new_revision: u64,
        delta: D,
    },
    Heartbeat {
        cursor: WatchCursor,
        current_revision: u64,
    },
    ResyncRequired {
        cursor: WatchCursor,
        current_revision: u64,
        reason: ResyncReason,
    },
    Unavailable {
        error: WatchError,
    },
    AccessRevoked,
}

#[must_use]
pub fn apply_frame<T: Clone, D>(
    current: WatchState<T>,
    frame: WatchFrame<T, D>,
    apply_delta: impl FnOnce(&T, D) -> Result<T, ()>,
) -> WatchState<T> {
    match frame {
        WatchFrame::Snapshot {
            cursor,
            revision,
            fingerprint,
            value,
        } => {
            let previous = current.cached();
            if cursor.sequence != 1 {
                return WatchState::Resyncing {
                    cached: previous,
                    reason: ResyncReason::SnapshotInvalid,
                };
            }
            if let Some(cached) = previous.as_ref() {
                if cursor.authority_epoch != cached.cursor.authority_epoch {
                    return WatchState::Stale {
                        cached: cached.clone(),
                        reason: ResyncReason::AuthorityEpochChanged,
                    };
                }
                if cursor.stream_id == cached.cursor.stream_id || revision < cached.revision {
                    return WatchState::Resyncing {
                        cached: previous,
                        reason: ResyncReason::SnapshotInvalid,
                    };
                }
            }
            WatchState::Fresh(CachedWatch {
                cursor,
                revision,
                fingerprint,
                value,
            })
        }
        WatchFrame::Delta {
            cursor,
            base_revision,
            new_revision,
            delta,
        } => {
            let WatchState::Fresh(cached) = current else {
                return current;
            };
            if cursor.authority_epoch != cached.cursor.authority_epoch {
                return WatchState::Stale {
                    cached,
                    reason: ResyncReason::AuthorityEpochChanged,
                };
            }
            if cursor.stream_id != cached.cursor.stream_id {
                return WatchState::Resyncing {
                    cached: Some(cached),
                    reason: ResyncReason::StreamReplaced,
                };
            }
            if cursor.sequence <= cached.cursor.sequence {
                return WatchState::Fresh(cached);
            }
            if cursor.sequence != cached.cursor.sequence + 1 {
                return WatchState::Resyncing {
                    cached: Some(cached),
                    reason: ResyncReason::SequenceGap,
                };
            }
            if base_revision != cached.revision || new_revision <= base_revision {
                return WatchState::Resyncing {
                    cached: Some(cached),
                    reason: ResyncReason::RevisionGap,
                };
            }
            let Ok(value) = apply_delta(&cached.value, delta) else {
                return WatchState::Resyncing {
                    cached: Some(cached),
                    reason: ResyncReason::SnapshotInvalid,
                };
            };
            WatchState::Fresh(CachedWatch {
                cursor,
                revision: new_revision,
                fingerprint: cached.fingerprint,
                value,
            })
        }
        WatchFrame::Heartbeat {
            cursor,
            current_revision,
        } => {
            let WatchState::Fresh(mut cached) = current else {
                return current;
            };
            if cursor.authority_epoch != cached.cursor.authority_epoch {
                return WatchState::Stale {
                    cached,
                    reason: ResyncReason::AuthorityEpochChanged,
                };
            }
            if cursor.stream_id != cached.cursor.stream_id
                || cursor.sequence != cached.cursor.sequence + 1
            {
                return WatchState::Resyncing {
                    cached: Some(cached),
                    reason: ResyncReason::SequenceGap,
                };
            }
            if current_revision != cached.revision {
                return WatchState::Resyncing {
                    cached: Some(cached),
                    reason: ResyncReason::RevisionGap,
                };
            }
            cached.cursor = cursor;
            WatchState::Fresh(cached)
        }
        WatchFrame::ResyncRequired { reason, .. } => WatchState::Resyncing {
            cached: current.cached(),
            reason,
        },
        WatchFrame::Unavailable { error } => WatchState::Unavailable {
            cached: current.cached(),
            error,
        },
        WatchFrame::AccessRevoked => WatchState::Revoked,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cursor(sequence: u64) -> WatchCursor {
        WatchCursor {
            stream_id: "stream-a".to_owned(),
            sequence,
            authority_epoch: 7,
        }
    }

    fn snapshot() -> WatchState<Vec<u8>> {
        apply_frame(
            WatchState::Empty,
            WatchFrame::<Vec<u8>, u8>::Snapshot {
                cursor: cursor(1),
                revision: 4,
                fingerprint: vec![9],
                value: vec![1],
            },
            |_, _| unreachable!(),
        )
    }

    #[test]
    fn test_watch_gap_resync_001_orders_deduplicates_and_blocks_after_gap() {
        let state = apply_frame(
            snapshot(),
            WatchFrame::Delta {
                cursor: cursor(2),
                base_revision: 4,
                new_revision: 5,
                delta: 2,
            },
            |value, delta| {
                let mut value = value.clone();
                value.push(delta);
                Ok(value)
            },
        );
        let duplicate = apply_frame(
            state.clone(),
            WatchFrame::Delta {
                cursor: cursor(2),
                base_revision: 4,
                new_revision: 5,
                delta: 99,
            },
            |_, _| panic!("duplicate delta must not be applied"),
        );
        assert_eq!(duplicate, state);

        let gap = apply_frame(
            state,
            WatchFrame::Delta {
                cursor: cursor(4),
                base_revision: 5,
                new_revision: 6,
                delta: 3,
            },
            |_, _| panic!("gap delta must not be applied"),
        );
        assert!(matches!(
            gap,
            WatchState::Resyncing {
                reason: ResyncReason::SequenceGap,
                ..
            }
        ));
        let still_blocked = apply_frame(
            gap.clone(),
            WatchFrame::Delta {
                cursor: cursor(3),
                base_revision: 5,
                new_revision: 6,
                delta: 3,
            },
            |_, _| panic!("delta must not apply until a new snapshot"),
        );
        assert_eq!(still_blocked, gap);
    }

    #[test]
    fn unavailable_preserves_cache_and_revoke_removes_it() {
        let unavailable = apply_frame(
            snapshot(),
            WatchFrame::<Vec<u8>, u8>::Unavailable {
                error: WatchError {
                    code: "offline".to_owned(),
                    message_key: "watch.offline".to_owned(),
                },
            },
            |_, _| unreachable!(),
        );
        assert!(unavailable.cached().is_some());
        let revoked = apply_frame(
            unavailable,
            WatchFrame::<Vec<u8>, u8>::AccessRevoked,
            |_, _| unreachable!(),
        );
        assert_eq!(revoked, WatchState::Revoked);
        assert!(revoked.cached().is_none());
    }

    #[test]
    fn stale_epoch_invalidates_delta() {
        let stale = apply_frame(
            snapshot(),
            WatchFrame::Delta {
                cursor: WatchCursor {
                    stream_id: "stream-a".to_owned(),
                    sequence: 2,
                    authority_epoch: 8,
                },
                base_revision: 4,
                new_revision: 5,
                delta: 1_u8,
            },
            |_, _| panic!("epoch-mismatched delta must not apply"),
        );
        assert!(matches!(
            stale,
            WatchState::Stale {
                reason: ResyncReason::AuthorityEpochChanged,
                ..
            }
        ));
    }

    #[test]
    fn new_epoch_snapshot_requires_handshake_reset() {
        let stale = apply_frame(
            snapshot(),
            WatchFrame::<Vec<u8>, u8>::Snapshot {
                cursor: WatchCursor {
                    stream_id: "stream-b".to_owned(),
                    sequence: 1,
                    authority_epoch: 8,
                },
                revision: 5,
                fingerprint: vec![10],
                value: vec![2],
            },
            |_, _| unreachable!(),
        );
        assert!(matches!(
            stale,
            WatchState::Stale {
                reason: ResyncReason::AuthorityEpochChanged,
                ..
            }
        ));
    }

    #[test]
    fn snapshot_must_open_a_new_stream_at_sequence_one() {
        for frame in [
            WatchFrame::<Vec<u8>, u8>::Snapshot {
                cursor: cursor(2),
                revision: 5,
                fingerprint: vec![10],
                value: vec![2],
            },
            WatchFrame::<Vec<u8>, u8>::Snapshot {
                cursor: cursor(1),
                revision: 5,
                fingerprint: vec![10],
                value: vec![2],
            },
        ] {
            assert!(matches!(
                apply_frame(snapshot(), frame, |_, _| unreachable!()),
                WatchState::Resyncing {
                    reason: ResyncReason::SnapshotInvalid,
                    ..
                }
            ));
        }
    }

    #[test]
    fn bounded_sequence_model_accepts_only_the_exact_successor() {
        for sequence in 2..16 {
            let state = apply_frame(
                snapshot(),
                WatchFrame::Delta {
                    cursor: cursor(sequence),
                    base_revision: 4,
                    new_revision: 5,
                    delta: 2_u8,
                },
                |value, delta| {
                    let mut value = value.clone();
                    value.push(delta);
                    Ok(value)
                },
            );
            if sequence == 2 {
                assert!(matches!(state, WatchState::Fresh(_)));
            } else {
                assert!(matches!(
                    state,
                    WatchState::Resyncing {
                        reason: ResyncReason::SequenceGap,
                        ..
                    }
                ));
            }
        }
    }
}
