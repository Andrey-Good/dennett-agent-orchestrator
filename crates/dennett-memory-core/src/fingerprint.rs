//! Canonical SHA-256 hashing and idempotency payload fingerprinting.
//!
//! Uses length-prefixed, domain-separated binary framing so distinct payloads can never
//! collide via string concatenation ambiguity and `BTreeMap` iteration stays deterministic.

use std::collections::BTreeMap;

use dennett_contracts::{
    CorrectNoteRequest, CreateNoteRequest, EvidenceAnchor, EvidenceLinkInput,
    IngestEvidenceRequest, Sha256Hex, TimestampMs,
};
use sha2::{Digest, Sha256};

/// Computes the lowercase 64-character hexadecimal SHA-256 digest of `bytes`.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> Sha256Hex {
    Sha256Hex::from_digest_bytes(sha256_bytes(bytes))
}

/// Computes the raw 32-byte SHA-256 digest of `bytes`.
#[must_use]
pub fn sha256_bytes(bytes: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(bytes);
    let mut out = [0_u8; 32];
    out.copy_from_slice(&digest);
    out
}

/// Computes a deterministic canonical fingerprint for [`IngestEvidenceRequest`].
#[must_use]
pub fn fingerprint_ingest_evidence(req: &IngestEvidenceRequest) -> Sha256Hex {
    let mut hasher = CanonicalHasher::new("dennett.memory.v1.evidence_ingested");
    hasher.update_str(req.scope.as_str());
    hasher.update_str(&req.media_type);
    hasher.update_str(&req.content);
    hasher.update_opt_str(req.source_uri.as_deref());
    hasher.update_opt_timestamp(req.observed_at);
    hasher.update_map(&req.metadata);
    hasher.finish()
}

/// Computes a deterministic canonical fingerprint for [`CreateNoteRequest`].
#[must_use]
pub fn fingerprint_create_note(req: &CreateNoteRequest) -> Sha256Hex {
    let mut hasher = CanonicalHasher::new("dennett.memory.v1.note_created");
    hasher.update_str(req.scope.as_str());
    hasher.update_str(&req.title);
    hasher.update_str(&req.body);
    hasher.update_str_slice(&req.tags);
    hasher.update_opt_timestamp(req.observed_at);
    hasher.update_opt_str(req.author_label.as_deref());
    hasher.update_links(&req.evidence_links);
    hasher.finish()
}

/// Computes a deterministic canonical fingerprint for [`CorrectNoteRequest`].
#[must_use]
pub fn fingerprint_correct_note(req: &CorrectNoteRequest) -> Sha256Hex {
    let mut hasher = CanonicalHasher::new("dennett.memory.v1.note_corrected");
    hasher.update_bytes(req.note_id.as_uuid().as_bytes());
    hasher.update_str(req.scope.as_str());
    hasher.update_i64(req.expected_revision);
    hasher.update_str(&req.title);
    hasher.update_str(&req.body);
    hasher.update_str(&req.rationale);
    hasher.update_str_slice(&req.tags);
    hasher.update_opt_str(req.author_label.as_deref());
    hasher.update_links(&req.evidence_links);
    hasher.finish()
}

struct CanonicalHasher {
    inner: Sha256,
}

impl CanonicalHasher {
    fn new(domain_tag: &str) -> Self {
        let mut inner = Sha256::new();
        let len = u64::try_from(domain_tag.len()).unwrap_or(u64::MAX);
        inner.update(len.to_be_bytes());
        inner.update(domain_tag.as_bytes());
        Self { inner }
    }

    fn update_bytes(&mut self, bytes: &[u8]) {
        let len = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        self.inner.update(len.to_be_bytes());
        self.inner.update(bytes);
    }

    fn update_str(&mut self, s: &str) {
        self.update_bytes(s.as_bytes());
    }

    fn update_opt_str(&mut self, opt: Option<&str>) {
        match opt {
            None => self.inner.update([0_u8]),
            Some(v) => {
                self.inner.update([1_u8]);
                self.update_str(v);
            }
        }
    }

    fn update_i64(&mut self, value: i64) {
        self.inner.update(value.to_be_bytes());
    }

    fn update_opt_timestamp(&mut self, opt: Option<TimestampMs>) {
        match opt {
            None => self.inner.update([0_u8]),
            Some(ts) => {
                self.inner.update([1_u8]);
                self.update_i64(ts.as_millis());
            }
        }
    }

    fn update_opt_u64(&mut self, opt: Option<u64>) {
        match opt {
            None => self.inner.update([0_u8]),
            Some(v) => {
                self.inner.update([1_u8]);
                self.inner.update(v.to_be_bytes());
            }
        }
    }

    fn update_opt_i32(&mut self, opt: Option<i32>) {
        match opt {
            None => self.inner.update([0_u8]),
            Some(v) => {
                self.inner.update([1_u8]);
                self.inner.update(v.to_be_bytes());
            }
        }
    }

    fn update_str_slice(&mut self, items: &[String]) {
        let count = u64::try_from(items.len()).unwrap_or(u64::MAX);
        self.inner.update(count.to_be_bytes());
        for item in items {
            self.update_str(item);
        }
    }

    fn update_map(&mut self, map: &BTreeMap<String, String>) {
        let count = u64::try_from(map.len()).unwrap_or(u64::MAX);
        self.inner.update(count.to_be_bytes());
        for (k, v) in map {
            self.update_str(k);
            self.update_str(v);
        }
    }

    fn update_anchor(&mut self, anchor: Option<&EvidenceAnchor>) {
        match anchor {
            None => self.inner.update([0_u8]),
            Some(a) => {
                self.inner.update([1_u8]);
                self.update_str(&a.kind);
                self.update_opt_u64(a.start);
                self.update_opt_u64(a.end);
                self.update_opt_str(a.locator.as_deref());
                self.update_map(&a.attributes);
            }
        }
    }

    fn update_links(&mut self, links: &[EvidenceLinkInput]) {
        let count = u64::try_from(links.len()).unwrap_or(u64::MAX);
        self.inner.update(count.to_be_bytes());
        for link in links {
            self.update_bytes(link.evidence_id.as_uuid().as_bytes());
            self.update_str(link.relationship.as_str());
            self.update_opt_i32(link.ordinal);
            self.update_anchor(link.anchor.as_ref());
        }
    }

    fn finish(self) -> Sha256Hex {
        let digest = self.inner.finalize();
        let mut bytes = [0_u8; 32];
        bytes.copy_from_slice(&digest);
        Sha256Hex::from_digest_bytes(bytes)
    }
}

#[cfg(test)]
mod tests {
    use dennett_contracts::{
        EvidenceId, EvidenceRelationship, IdempotencyKey, MemoryNoteId, MemoryScope,
    };
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn fingerprint_distinguishes_concatenation_boundaries_and_anchors() {
        let Ok(scope) = MemoryScope::try_new("project-alpha") else {
            panic!("valid scope");
        };
        let Ok(key) = IdempotencyKey::try_new("op-1") else {
            panic!("valid key");
        };
        let req_a = CreateNoteRequest {
            scope: scope.clone(),
            title: String::from("ab"),
            body: String::from("c"),
            tags: vec![],
            observed_at: None,
            evidence_links: vec![],
            author_label: None,
            idempotency_key: key.clone(),
        };
        let req_b = CreateNoteRequest {
            title: String::from("a"),
            body: String::from("bc"),
            ..req_a.clone()
        };
        assert_ne!(
            fingerprint_create_note(&req_a),
            fingerprint_create_note(&req_b),
            "length-prefixed framing must prevent concatenation collisions"
        );

        let ev_id = EvidenceId::new();
        let Ok(rel) = EvidenceRelationship::try_new("supports") else {
            panic!("valid rel");
        };
        let with_anchor_1 = CreateNoteRequest {
            evidence_links: vec![EvidenceLinkInput {
                evidence_id: ev_id,
                relationship: rel.clone(),
                anchor: Some(EvidenceAnchor {
                    kind: String::from("line_range"),
                    start: Some(1),
                    end: Some(5),
                    locator: None,
                    attributes: BTreeMap::new(),
                }),
                ordinal: None,
            }],
            ..req_a.clone()
        };
        let with_anchor_2 = CreateNoteRequest {
            evidence_links: vec![EvidenceLinkInput {
                evidence_id: ev_id,
                relationship: rel,
                anchor: Some(EvidenceAnchor {
                    kind: String::from("line_range"),
                    start: Some(1),
                    end: Some(6),
                    locator: None,
                    attributes: BTreeMap::new(),
                }),
                ordinal: None,
            }],
            ..req_a
        };
        assert_ne!(
            fingerprint_create_note(&with_anchor_1),
            fingerprint_create_note(&with_anchor_2)
        );
    }

    #[test]
    fn correction_fingerprint_changes_with_expected_revision_and_rationale() {
        let Ok(scope) = MemoryScope::try_new("project-alpha") else {
            panic!("valid scope");
        };
        let Ok(key) = IdempotencyKey::try_new("corr-1") else {
            panic!("valid key");
        };
        let base = CorrectNoteRequest {
            note_id: MemoryNoteId::new(),
            scope,
            expected_revision: 1,
            title: String::from("Decision"),
            body: String::from("Use PG 16"),
            tags: vec![String::from("arch")],
            rationale: String::from("Updated policy"),
            evidence_links: vec![],
            author_label: None,
            idempotency_key: key,
        };
        let diff_rev = CorrectNoteRequest {
            expected_revision: 2,
            ..base.clone()
        };
        let diff_rationale = CorrectNoteRequest {
            rationale: String::from("Different reason"),
            ..base.clone()
        };
        assert_ne!(
            fingerprint_correct_note(&base),
            fingerprint_correct_note(&diff_rev)
        );
        assert_ne!(
            fingerprint_correct_note(&base),
            fingerprint_correct_note(&diff_rationale)
        );
    }

    proptest! {
        #[test]
        fn prop_create_note_fingerprint_is_deterministic_and_sensitive_to_body(
            title in "[A-Za-z0-9 _-]{1,40}",
            body_a in "[A-Za-z0-9 _-]{1,64}",
            body_b in "[A-Za-z0-9 _-]{1,64}",
        ) {
            prop_assume!(body_a != body_b);
            let Ok(scope) = MemoryScope::try_new("project-prop") else {
                unreachable!();
            };
            let Ok(key) = IdempotencyKey::try_new("prop-key") else {
                unreachable!();
            };
            let req_a = CreateNoteRequest {
                scope: scope.clone(),
                title: title.clone(),
                body: body_a,
                tags: vec![],
                observed_at: None,
                evidence_links: vec![],
                author_label: None,
                idempotency_key: key.clone(),
            };
            let req_b = CreateNoteRequest {
                body: body_b,
                ..req_a.clone()
            };
            prop_assert_eq!(fingerprint_create_note(&req_a), fingerprint_create_note(&req_a));
            prop_assert_ne!(fingerprint_create_note(&req_a), fingerprint_create_note(&req_b));
        }
    }
}
