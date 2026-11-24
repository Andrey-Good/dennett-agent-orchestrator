//! Single canonical, UTF-8-safe, idempotent secret redaction module for Dennett.
//!
//! Uses only the Rust standard library so every crate in the workspace (`dennett-contracts`,
//! `dennett-memoryd`, `dennett-testkit`) shares the exact same credential scrubbing rules.

/// Replacement marker inserted whenever a secret value or credential pattern is redacted.
pub const REDACTED_PLACEHOLDER: &str = "[REDACTED]";

/// Minimum byte length required before a password auto-extracted from a PostgreSQL DSN is
/// globally replaced across arbitrary error text (shorter passwords are still scrubbed by
/// URI and key-value rules without corrupting unrelated English words).
const MIN_AUTO_GLOBAL_SECRET_LEN: usize = 3;

const SECRET_KEY_NAMES: [&str; 15] = [
    "password",
    "gemini_api_key",
    "google_api_key",
    "x-goog-api-key",
    "access_token",
    "refresh_token",
    "dennett_memory_token",
    "memory_token",
    "dennett_head_token",
    "head_token",
    "dennett_control_token",
    "control_token",
    "x-dennett-control-token",
    "auth_secret",
    "hmac_secret",
];

/// Redacts explicit `extra_secrets` and common credential patterns from `input`.
///
/// Guarantees:
/// - Never panics on arbitrary UTF-8 (including multi-byte characters immediately after `\`).
/// - Strictly idempotent: `redact_secrets(&redact_secrets(s, k), k) == redact_secrets(s, k)`.
/// - Scrubs URI userinfo (including punctuated passwords), libpq/query/CLI `--key=val`
///   and `key: val` secrets, `Bearer` tokens (with arbitrary spaces/tabs), Google API keys
///   (`AIza...`), OAuth access/refresh tokens (`ya29....`, `1//...`), and JWT tokens.
#[must_use]
pub fn redact_secrets(input: &str, extra_secrets: &[&str]) -> String {
    let mut ordered: Vec<&str> = extra_secrets
        .iter()
        .copied()
        .filter(|s| !s.is_empty() && *s != REDACTED_PLACEHOLDER)
        .collect();
    ordered.sort_by_key(|s| std::cmp::Reverse(s.len()));

    // Run to a bounded fixpoint so that replacing an explicit `extra_secret` adjacent to a
    // secret key (for example `"cGEMINI_API_KEY="` with extra `"c"` -> `"[REDACTED]GEMINI_API_KEY="`)
    // also applies structural redaction on the first call, preserving strict idempotence.
    let mut out = input.to_owned();
    for _ in 0..4 {
        let next = redact_once(&out, &ordered);
        if next == out {
            break;
        }
        out = next;
    }
    out
}

fn redact_once(input: &str, ordered_extra_secrets: &[&str]) -> String {
    // 1. Apply structural redaction first so key/URI/token patterns produce clean
    //    `[REDACTED]` spans before substring replacement of `extra_secrets`.
    let mut out = redact_uri_userinfo(input);
    for key in SECRET_KEY_NAMES {
        out = redact_assignment_key(&out, key);
        out = redact_colon_key(&out, key);
    }
    out = redact_bearer_tokens(&out);
    out = redact_prefixed_token(&out, "AIza", 20, is_google_key_char);
    out = redact_prefixed_token(&out, "ya29.", 10, is_oauth_token_char);
    out = redact_prefixed_token(&out, "1//", 10, is_oauth_token_char);
    out = redact_dennett_signed_tokens(&out);
    out = redact_jwt_tokens(&out);

    // 2. Replace explicit caller-supplied secrets strictly outside `[REDACTED]` spans.
    for secret in ordered_extra_secrets {
        out = replace_outside_redacted(&out, secret);
    }
    out
}

/// Convenience wrapper around [`redact_secrets`] for owned [`String`] slices.
#[must_use]
pub fn redact_secrets_owned(input: &str, extra_secrets: &[String]) -> String {
    let borrowed: Vec<&str> = extra_secrets.iter().map(String::as_str).collect();
    redact_secrets(input, &borrowed)
}

/// Redacts credentials embedded in `pg_url` and generic secret patterns from `raw_error`.
#[must_use]
pub fn sanitize_pg_error(raw_error: &str, pg_url: Option<&str>) -> String {
    sanitize_pg_error_with_password(raw_error, pg_url, None)
}

/// Redacts credentials extracted from `pg_url`, an optional driver-parsed password (for
/// example from `tokio_postgres::Config::get_password`), and generic secret patterns.
///
/// Auto-extracted passwords shorter than 3 bytes are excluded from global substring
/// replacement so 1-2 character passwords do not corrupt unrelated diagnostic text while
/// still being scrubbed inside URI/key-value positions.
#[must_use]
pub fn sanitize_pg_error_with_password(
    raw_error: &str,
    pg_url: Option<&str>,
    parsed_password: Option<&str>,
) -> String {
    let mut secrets = Vec::new();
    if let Some(url) = pg_url {
        for item in extract_connection_secrets(url) {
            if item.len() >= MIN_AUTO_GLOBAL_SECRET_LEN {
                secrets.push(item);
            }
        }
    }
    if let Some(pw) = parsed_password
        && pw.len() >= MIN_AUTO_GLOBAL_SECRET_LEN
    {
        secrets.push(pw.to_owned());
    }
    redact_secrets_owned(raw_error, &secrets)
}

/// Sanitizes an unrecognized CLI argument before echoing it in an error message.
///
/// Any `--flag=value` argument has its value stripped after `=`, and the entire string is
/// also passed through [`redact_secrets`] so bare connection URIs or tokens never leak.
#[must_use]
pub fn sanitize_cli_arg(arg: &str) -> String {
    let truncated = match arg.split_once('=') {
        Some((flag, _value)) => format!("{flag}={REDACTED_PLACEHOLDER}"),
        None => arg.to_owned(),
    };
    redact_secrets(&truncated, &[])
}

/// Extracts password-bearing fragments from a PostgreSQL connection string so callers
/// can feed them into [`redact_secrets_owned`] alongside other secrets.
#[must_use]
pub fn extract_connection_secrets(conn_str: &str) -> Vec<String> {
    let mut secrets = Vec::new();
    let trimmed = conn_str.trim();
    if trimmed.is_empty() {
        return secrets;
    }

    if let Some(scheme_pos) = trimmed.find("://") {
        let after_scheme = &trimmed[scheme_pos + 3..];
        let token_end = after_scheme
            .find(char::is_whitespace)
            .unwrap_or(after_scheme.len());
        let span = &after_scheme[..token_end];
        if let Some(at_pos) = find_userinfo_at(span) {
            let userinfo = &span[..at_pos];
            if let Some(colon_pos) = userinfo.find(':') {
                let raw_pass = &userinfo[colon_pos + 1..];
                push_secret_variants(&mut secrets, raw_pass);
            }
        }
    }

    for val in extract_assignment_values(trimmed, "password") {
        push_secret_variants(&mut secrets, &val);
    }

    secrets
}

fn replace_outside_redacted(input: &str, secret: &str) -> String {
    if !input.contains(REDACTED_PLACEHOLDER) {
        return input.replace(secret, REDACTED_PLACEHOLDER);
    }
    let mut out = String::with_capacity(input.len());
    for (idx, segment) in input.split(REDACTED_PLACEHOLDER).enumerate() {
        if idx > 0 {
            out.push_str(REDACTED_PLACEHOLDER);
        }
        out.push_str(&segment.replace(secret, REDACTED_PLACEHOLDER));
    }
    out
}

fn push_secret_variants(dst: &mut Vec<String>, raw: &str) {
    if raw.is_empty() || raw == REDACTED_PLACEHOLDER {
        return;
    }
    dst.push(raw.to_owned());
    let decoded = percent_decode(raw);
    if decoded != raw && !decoded.is_empty() {
        dst.push(decoded);
    }
    let unescaped = raw.replace("\\'", "'").replace("''", "'");
    if unescaped != raw && !unescaped.is_empty() {
        dst.push(unescaped);
    }
}

fn find_userinfo_at(span: &str) -> Option<usize> {
    let bytes = span.as_bytes();
    for idx in (0..bytes.len()).rev() {
        if bytes[idx] != b'@' {
            continue;
        }
        let after = &span[idx + 1..];
        let host_starts_ok = after
            .as_bytes()
            .first()
            .is_some_and(|&b| b.is_ascii_alphanumeric() || matches!(b, b'[' | b'-' | b'.'));
        if !host_starts_ok {
            continue;
        }
        let before = &span[..idx];
        let has_delim = before.contains(['/', '?', '#']);
        if !has_delim || before.contains(':') {
            return Some(idx);
        }
    }
    None
}

fn redact_uri_userinfo(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut rest = input;

    while let Some(idx) = rest.find("://") {
        let prefix_end = idx + 3;
        result.push_str(&rest[..prefix_end]);
        rest = &rest[prefix_end..];

        let token_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let span = &rest[..token_end];

        if let Some(at_pos) = find_userinfo_at(span) {
            result.push_str(REDACTED_PLACEHOLDER);
            result.push_str(&span[at_pos..]);
        } else {
            result.push_str(span);
        }
        rest = &rest[token_end..];
    }

    result.push_str(rest);
    result
}

/// Checks whether `pos` starts a standalone key or a CLI long flag (`--<key>`).
fn is_word_boundary_left(bytes: &[u8], pos: usize) -> bool {
    if pos == 0 {
        return true;
    }
    let prev = bytes[pos - 1];
    if !(prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'-') {
        return true;
    }
    // Allow CLI long flags like `--password=...` or `--gemini_api_key=...`.
    if pos >= 2 && bytes[pos - 1] == b'-' && bytes[pos - 2] == b'-' {
        if pos == 2 {
            return true;
        }
        let before_dashes = bytes[pos - 3];
        return !(before_dashes.is_ascii_alphanumeric()
            || before_dashes == b'_'
            || before_dashes == b'-');
    }
    false
}

/// Scans a single- or double-quoted value safely across arbitrary UTF-8 characters.
fn scan_quoted_value(rest: &str, quote: char) -> (usize, String) {
    let mut chars = rest.char_indices();
    // Skip opening quote.
    let _ = chars.next();

    let mut inner = String::new();
    while let Some((byte_idx, ch)) = chars.next() {
        if ch == '\\' {
            if let Some((_, escaped)) = chars.next() {
                inner.push(escaped);
            }
            continue;
        }
        if ch == quote {
            if quote == '\'' && chars.clone().next().is_some_and(|(_, next)| next == '\'') {
                let _ = chars.next();
                inner.push('\'');
                continue;
            }
            return (byte_idx + ch.len_utf8(), inner);
        }
        inner.push(ch);
    }
    (rest.len(), inner)
}

/// Scans an unquoted value while preserving `[REDACTED]` as an atomic token boundary
/// so repeated calls remain strictly idempotent even on adjacent `password=...password=...`.
fn scan_unquoted_value(rest: &str) -> usize {
    if rest.starts_with(REDACTED_PLACEHOLDER) {
        return REDACTED_PLACEHOLDER.len();
    }
    let mut pos = 0;
    while pos < rest.len() {
        let tail = &rest[pos..];
        if tail.starts_with(REDACTED_PLACEHOLDER) {
            break;
        }
        let Some(ch) = tail.chars().next() else {
            break;
        };
        if ch.is_whitespace() || matches!(ch, '&' | ';' | ',' | '"' | '\'' | ')' | '}' | ']') {
            break;
        }
        pos += ch.len_utf8();
    }
    pos
}

fn scan_bearer_token(rest: &str) -> usize {
    if rest.starts_with(REDACTED_PLACEHOLDER) {
        return REDACTED_PLACEHOLDER.len();
    }
    let mut pos = 0;
    while pos < rest.len() {
        let tail = &rest[pos..];
        if tail.starts_with(REDACTED_PLACEHOLDER) {
            break;
        }
        let Some(ch) = tail.chars().next() else {
            break;
        };
        if ch.is_whitespace() || matches!(ch, '"' | '\'' | ',' | ';' | ')' | '}' | ']') {
            break;
        }
        pos += ch.len_utf8();
    }
    pos
}

fn redact_assignment_key(input: &str, lowercase_key: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut idx = 0;
    let lower = input.to_ascii_lowercase();
    let bytes = input.as_bytes();

    while let Some(rel) = lower[idx..].find(lowercase_key) {
        let key_start = idx + rel;
        let after_key = key_start + lowercase_key.len();

        if !is_word_boundary_left(bytes, key_start) {
            result.push_str(&input[idx..after_key]);
            idx = after_key;
            continue;
        }

        let mut cursor = after_key;
        while bytes.get(cursor).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            cursor += 1;
        }
        if bytes.get(cursor) != Some(&b'=') {
            result.push_str(&input[idx..after_key]);
            idx = after_key;
            continue;
        }
        cursor += 1;
        while bytes.get(cursor).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            cursor += 1;
        }

        result.push_str(&input[idx..cursor]);
        result.push_str(REDACTED_PLACEHOLDER);

        let val_rest = &input[cursor..];
        let consumed = match val_rest.as_bytes().first() {
            Some(&b'\'') => scan_quoted_value(val_rest, '\'').0,
            Some(&b'"') => scan_quoted_value(val_rest, '"').0,
            _ => scan_unquoted_value(val_rest),
        };
        idx = cursor + consumed;
    }

    result.push_str(&input[idx..]);
    result
}

fn extract_assignment_values(input: &str, lowercase_key: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut idx = 0;
    let lower = input.to_ascii_lowercase();
    let bytes = input.as_bytes();

    while let Some(rel) = lower[idx..].find(lowercase_key) {
        let key_start = idx + rel;
        let after_key = key_start + lowercase_key.len();
        if !is_word_boundary_left(bytes, key_start) {
            idx = after_key;
            continue;
        }

        let mut cursor = after_key;
        while bytes.get(cursor).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            cursor += 1;
        }
        if bytes.get(cursor) != Some(&b'=') {
            idx = after_key;
            continue;
        }
        cursor += 1;
        while bytes.get(cursor).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            cursor += 1;
        }

        let val_rest = &input[cursor..];
        let (consumed, val) = match val_rest.as_bytes().first() {
            Some(&b'\'') => scan_quoted_value(val_rest, '\''),
            Some(&b'"') => scan_quoted_value(val_rest, '"'),
            _ => {
                let len = scan_unquoted_value(val_rest);
                (len, val_rest[..len].to_owned())
            }
        };
        if !val.is_empty() && val != REDACTED_PLACEHOLDER {
            out.push(val);
        }
        idx = cursor + consumed;
    }
    out
}

fn redact_colon_key(input: &str, lowercase_key: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut idx = 0;
    let lower = input.to_ascii_lowercase();
    let bytes = input.as_bytes();

    while let Some(rel) = lower[idx..].find(lowercase_key) {
        let key_start = idx + rel;
        let after_key = key_start + lowercase_key.len();

        if !is_word_boundary_left(bytes, key_start) {
            result.push_str(&input[idx..after_key]);
            idx = after_key;
            continue;
        }

        let mut cursor = after_key;
        if bytes.get(cursor).is_some_and(|b| matches!(b, b'"' | b'\'')) {
            cursor += 1;
        }
        while bytes.get(cursor).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            cursor += 1;
        }
        if bytes.get(cursor) != Some(&b':') {
            result.push_str(&input[idx..after_key]);
            idx = after_key;
            continue;
        }
        cursor += 1;
        while bytes.get(cursor).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            cursor += 1;
        }

        result.push_str(&input[idx..cursor]);
        let val_rest = &input[cursor..];
        let consumed = match val_rest.as_bytes().first() {
            Some(&b'"') => {
                result.push('"');
                result.push_str(REDACTED_PLACEHOLDER);
                result.push('"');
                scan_quoted_value(val_rest, '"').0
            }
            Some(&b'\'') => {
                result.push('\'');
                result.push_str(REDACTED_PLACEHOLDER);
                result.push('\'');
                scan_quoted_value(val_rest, '\'').0
            }
            _ => {
                result.push_str(REDACTED_PLACEHOLDER);
                scan_unquoted_value(val_rest)
            }
        };
        idx = cursor + consumed;
    }

    result.push_str(&input[idx..]);
    result
}

fn redact_bearer_tokens(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut idx = 0;
    let lower = input.to_ascii_lowercase();
    let bytes = input.as_bytes();

    while let Some(rel) = lower[idx..].find("bearer") {
        let start = idx + rel;
        let after_word = start + "bearer".len();
        if !is_word_boundary_left(bytes, start) {
            result.push_str(&input[idx..after_word]);
            idx = after_word;
            continue;
        }

        let mut cursor = after_word;
        while bytes.get(cursor).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            cursor += 1;
        }
        if cursor == after_word {
            result.push_str(&input[idx..after_word]);
            idx = after_word;
            continue;
        }

        result.push_str(&input[idx..cursor]);
        let rest = &input[cursor..];
        let token_len = scan_bearer_token(rest);
        if token_len > 0 {
            result.push_str(REDACTED_PLACEHOLDER);
        }
        idx = cursor + token_len;
    }

    result.push_str(&input[idx..]);
    result
}

fn redact_prefixed_token(
    input: &str,
    prefix: &str,
    min_tail_len: usize,
    is_char: fn(u8) -> bool,
) -> String {
    let mut result = String::with_capacity(input.len());
    let mut idx = 0;

    while let Some(rel) = input[idx..].find(prefix) {
        let start = idx + rel;
        let tail_start = start + prefix.len();
        let tail = &input.as_bytes()[tail_start..];
        let tail_len = tail.iter().take_while(|&&b| is_char(b)).count();
        if tail_len >= min_tail_len {
            result.push_str(&input[idx..start]);
            result.push_str(REDACTED_PLACEHOLDER);
            idx = tail_start + tail_len;
        } else {
            result.push_str(&input[idx..tail_start]);
            idx = tail_start;
        }
    }

    result.push_str(&input[idx..]);
    result
}

fn redact_dennett_signed_tokens(input: &str) -> String {
    let after_dnt = redact_dennett_signed_token_prefix(input, "dnt1");
    redact_dennett_signed_token_prefix(&after_dnt, "dct1")
}

fn redact_dennett_signed_token_prefix(input: &str, prefix: &str) -> String {
    let needle = format!("{prefix}.");
    let mut result = String::with_capacity(input.len());
    let mut idx = 0;

    while let Some(rel) = input[idx..].find(&needle) {
        let start = idx + rel;
        let bytes = &input.as_bytes()[start..];
        let raw_len = bytes.iter().take_while(|&&b| is_jwt_char(b)).count();
        let trimmed = input[start..start + raw_len].trim_end_matches('.');
        let mut parts = trimmed.split('.');
        let is_signed = matches!(
            (parts.next(), parts.next(), parts.next(), parts.next()),
            (Some(p), Some(payload), Some(sig), None)
                if p == prefix && payload.len() >= 8 && sig.len() >= 8
        );

        if is_signed {
            result.push_str(&input[idx..start]);
            result.push_str(REDACTED_PLACEHOLDER);
            idx = start + trimmed.len();
        } else {
            result.push_str(&input[idx..start + needle.len()]);
            idx = start + needle.len();
        }
    }

    result.push_str(&input[idx..]);
    result
}

fn redact_jwt_tokens(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut idx = 0;

    while let Some(rel) = input[idx..].find("eyJ") {
        let start = idx + rel;
        let bytes = &input.as_bytes()[start..];
        let token_len = bytes.iter().take_while(|&&b| is_jwt_char(b)).count();
        let candidate = &input[start..start + token_len];
        let mut parts = candidate.split('.');
        let is_jwt = matches!(
            (parts.next(), parts.next(), parts.next(), parts.next()),
            (Some(a), Some(b), Some(c), None)
                if a.len() >= 8 && b.len() >= 8 && c.len() >= 8 && b.starts_with("eyJ")
        );

        if is_jwt {
            result.push_str(&input[idx..start]);
            result.push_str(REDACTED_PLACEHOLDER);
            idx = start + token_len;
        } else {
            result.push_str(&input[idx..start + 3]);
            idx = start + 3;
        }
    }

    result.push_str(&input[idx..]);
    result
}

const fn is_google_key_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

const fn is_oauth_token_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')
}

const fn is_jwt_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.')
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2]))
        {
            out.push((hi << 4) | lo);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

const fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn h0_01_escaped_multibyte_utf8_never_panics() {
        let single_quoted = redact_secrets("password='\\я'", &[]);
        assert_eq!(single_quoted, "password=[REDACTED]");

        let double_quoted = redact_secrets("password=\"\\🔥secret\"", &[]);
        assert_eq!(double_quoted, "password=[REDACTED]");

        let cli = sanitize_cli_arg("--serve=\"password='\\я'\"");
        assert!(
            cli.starts_with("--serve=[REDACTED]"),
            "expected safe CLI redaction, got: {cli}"
        );

        let bare = redact_secrets("password='\\я", &[]);
        assert_eq!(bare, "password=[REDACTED]");
    }

    #[test]
    fn h0_03_strict_idempotence_with_redacted_markers_and_short_extra_secrets() {
        let cases = [
            ("password=foo", &["E"][..]),
            ("password=foo", &["RED"][..]),
            ("password=[REDACTED]", &["E"][..]),
            ("password='\\я'password=", &["E"][..]),
            ("Authorization: Bearer [REDACTED]", &["E"][..]),
            ("password: foo", &["D"][..]),
            ("{\"access_token\": \"secret\"}", &["E"][..]),
            ("password=foo&sslmode=disable", &["fo"][..]),
            ("cGEMINI_API_KEY=", &["c"][..]),
        ];
        for (input, extra) in cases {
            let once = redact_secrets(input, extra);
            let twice = redact_secrets(&once, extra);
            assert_eq!(
                once, twice,
                "redact_secrets must be strictly idempotent for input={input:?}, extra={extra:?}"
            );
            assert!(
                !once.contains("]]"),
                "redaction must not duplicate closing bracket: {once}"
            );
        }

        // Single-character auto-extracted PG password must not corrupt unrelated text.
        let url = "postgres://u:a@127.0.0.1:55432/dennett_memory_test";
        let err = "authentication failed for postgres://u:a@127.0.0.1:55432/dennett_memory_test";
        let cleaned = sanitize_pg_error_with_password(err, Some(url), Some("a"));
        assert!(
            cleaned.contains("authentication failed"),
            "short auto-extracted password must not corrupt diagnostic words: {cleaned}"
        );
        assert!(cleaned.contains("postgres://[REDACTED]@127.0.0.1:55432/dennett_memory_test"));
    }

    #[test]
    fn h0_04_redacts_cli_double_dash_flags() {
        let cmd = "dennett --password=MY_SECRET --gemini_api_key=KEY_123 --access_token=TOK_456";
        let cleaned = redact_secrets(cmd, &[]);
        assert_eq!(
            cleaned,
            "dennett --password=[REDACTED] --gemini_api_key=[REDACTED] --access_token=[REDACTED]"
        );
        assert_eq!(redact_secrets(&cleaned, &[]), cleaned);
    }

    #[test]
    fn redacts_uri_kv_query_oauth_header_and_json_secrets() {
        let sample = "header x-goog-api-key: gkey_secret_111 \
                      env GOOGLE_API_KEY = gkey_secret_222 \
                      py {'GEMINI_API_KEY': 'py_gemini_333'} \
                      json {\"access_token\": \"acc_tok_444\", \"refresh_token\": \"ref_tok_555\"} \
                      refresh 1//0gRefreshTokenExample987654321 \
                      bearer1 Authorization: Bearer   multi_space_666 \
                      bearer2 Authorization: Bearer\ttab_tok_777 \
                      libpq password = spaced_pw_888 password='a\\'escaped_pw_999' \
                      uri postgres://u:p;a,r)a]m}s>'/uri_pw_000@127.0.0.1:55432/db?password=q_pw_101";

        let cleaned = redact_secrets(sample, &[]);

        for secret in [
            "gkey_secret_111",
            "gkey_secret_222",
            "py_gemini_333",
            "acc_tok_444",
            "ref_tok_555",
            "0gRefreshTokenExample987654321",
            "multi_space_666",
            "tab_tok_777",
            "spaced_pw_888",
            "escaped_pw_999",
            "uri_pw_000",
            "q_pw_101",
        ] {
            assert!(
                !cleaned.contains(secret),
                "secret `{secret}` leaked in redacted output: {cleaned}"
            );
        }
        assert!(cleaned.contains("127.0.0.1:55432/db"));
        assert_eq!(redact_secrets(&cleaned, &[]), cleaned);
    }

    #[test]
    fn sanitize_cli_arg_strips_flag_value_and_bare_uri_secret() {
        let flag_err = sanitize_cli_arg("--pg-url=postgres://u:CLI_SECRET_9@127.0.0.1/db");
        assert!(!flag_err.contains("CLI_SECRET_9"));
        assert!(flag_err.starts_with("--pg-url="));

        let bare_err = sanitize_cli_arg("postgres://u:BARE_SECRET_8@127.0.0.1/db");
        assert!(!bare_err.contains("BARE_SECRET_8"));
    }

    #[test]
    fn redact_secrets_scrubs_dennett_dnt1_tokens_idempotently() {
        let token = "dnt1.eyJ0b2tlbl9pZCI6ImhlYWQtMSIsInNjb3BlIjoicHJvamVjdDpkZW1vIn0.AbCdEf0123456789_-xyz9876543210";
        let raw = format!("memoryd rejected {token}. retry with header token={token}");
        let once = redact_secrets(&raw, &[]);
        assert!(!once.contains("dnt1."));
        assert!(!once.contains("eyJ0b2tlbl9pZCI6"));
        assert!(!once.contains("AbCdEf0123456789"));
        assert_eq!(
            once,
            "memoryd rejected [REDACTED]. retry with header token=[REDACTED]"
        );
        // Strict idempotence (h0_03)
        assert_eq!(redact_secrets(&once, &[]), once);
        // Non-token prefix stays intact
        assert_eq!(redact_secrets("dnt1.short.x", &[]), "dnt1.short.x");
    }

    #[test]
    fn redact_secrets_scrubs_dennett_dct1_control_tokens_idempotently() {
        let token = "dct1.eyJwcm9qZWN0X2lkIjoiYWJjZDEyMzQiLCJzZXNzaW9uX2tpbmQiOiJ1c2VyX2NoYXQifQ.XyZ9876543210_-abcdef0123456789";
        let raw = format!(
            "head rejected X-Dennett-Control-Token: {token} env DENNETT_CONTROL_TOKEN={token} raw={token}."
        );
        let once = redact_secrets(&raw, &[]);
        assert!(!once.contains("dct1."));
        assert!(!once.contains("eyJwcm9qZWN0X2lk"));
        assert!(!once.contains("XyZ9876543210"));
        assert_eq!(redact_secrets(&once, &[]), once);
        assert_eq!(redact_secrets("dct1.short.x", &[]), "dct1.short.x");
    }

    fn targeted_redaction_strategy() -> impl Strategy<Value = String> {
        prop::collection::vec(
            prop_oneof![
                Just(String::from("password=")),
                Just(String::from("--password=")),
                Just(String::from("password = ")),
                Just(String::from("password='\\я'")),
                Just(String::from("password=\"\\🔥\"")),
                Just(String::from("Bearer ")),
                Just(String::from("Bearer\t")),
                Just(String::from("[REDACTED]")),
                Just(String::from("dnt1.eyJ0ZXN0IjoxfQ.0123456789abcdef")),
                Just(String::from("dct1.eyJ0ZXN0IjoxfQ.0123456789abcdef")),
                Just(String::from("DENNETT_CONTROL_TOKEN=")),
                Just(String::from("X-Dennett-Control-Token: ")),
                Just(String::from("GEMINI_API_KEY=")),
                Just(String::from("?password=")),
                Just(String::from("&sslmode=disable")),
                "[A-Za-z0-9_я🔥'\"\\\\:;\\]\\[ ]{0,12}",
            ],
            1..8,
        )
        .prop_map(|parts| parts.concat())
    }

    proptest! {
        #[test]
        fn prop_targeted_redaction_is_strictly_idempotent(
            raw in targeted_redaction_strategy(),
            extra in prop_oneof![
                Just(String::from("E")),
                Just(String::from("RED")),
                Just(String::from("secret")),
                "[A-Za-z0-9_]{0,6}",
            ],
        ) {
            let once = redact_secrets(&raw, &[extra.as_str()]);
            let twice = redact_secrets(&once, &[extra.as_str()]);
            prop_assert_eq!(once, twice);
        }

        #[test]
        fn prop_sanitize_pg_error_never_leaks_uri_password(
            user in "[a-z][a-z0-9_]{1,12}",
            pass in "[A-Za-z0-9!#$%^&*()_+\\-=;,.>'/]{8,32}",
        ) {
            let url = format!("postgres://{user}:{pass}@127.0.0.1:55432/dennett_memory_test");
            let msg = format!("connection failed for {url} (retry)");
            let cleaned_with_url = sanitize_pg_error(&msg, Some(&url));
            let cleaned_without_url = sanitize_pg_error(&msg, None);
            prop_assert!(!cleaned_with_url.contains(&pass));
            prop_assert!(!cleaned_without_url.contains(&pass));
            prop_assert!(cleaned_without_url.contains("127.0.0.1:55432/dennett_memory_test"));
            prop_assert_eq!(
                sanitize_pg_error(&cleaned_without_url, None),
                cleaned_without_url
            );
        }

        #[test]
        fn prop_sanitize_cli_arg_never_leaks_flag_value(
            flag in "[a-z][a-z0-9-]{1,16}",
            secret in "[A-Za-z0-9_!@#:/-я]{8,32}",
        ) {
            let arg = format!("--{flag}={secret}");
            let cleaned = sanitize_cli_arg(&arg);
            prop_assert!(!cleaned.contains(&secret));
        }

        #[test]
        fn prop_redact_arbitrary_unicode_never_panics_and_is_idempotent(
            raw in any::<String>(),
            extra in any::<String>(),
        ) {
            let once = redact_secrets(&raw, &[extra.as_str()]);
            let twice = redact_secrets(&once, &[extra.as_str()]);
            prop_assert_eq!(once, twice);
        }
    }
}
