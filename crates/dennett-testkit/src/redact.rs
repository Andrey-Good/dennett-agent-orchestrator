//! Secret redaction for logs, error diagnostics, and protocol transcripts.

/// Replacement marker inserted whenever a secret value or credential pattern is redacted.
pub const REDACTED_PLACEHOLDER: &str = "[REDACTED]";

const SECRET_KEY_NAMES: [&str; 6] = [
    "password",
    "gemini_api_key",
    "google_api_key",
    "x-goog-api-key",
    "access_token",
    "refresh_token",
];

/// Redacts explicit secret values and common credential patterns (URI userinfo including
/// punctuated passwords, `password=`/`password = ` with escaped quotes, query passwords,
/// `Bearer` tokens with spaces/tabs, `GEMINI_API_KEY`/`GOOGLE_API_KEY`/`x-goog-api-key`,
/// JSON/YAML `access_token`/`refresh_token`, Google API keys `AIza...`, OAuth access
/// tokens `ya29....`, OAuth refresh tokens `1//...`, and JWT tokens) from `text`.
#[must_use]
pub fn redact_secrets(text: &str, secrets: &[&str]) -> String {
    let mut ordered: Vec<&str> = secrets.iter().copied().filter(|s| !s.is_empty()).collect();
    ordered.sort_by_key(|s| std::cmp::Reverse(s.len()));

    let mut out = text.to_owned();
    for secret in ordered {
        out = out.replace(secret, REDACTED_PLACEHOLDER);
    }

    out = redact_uri_userinfo(&out);
    for key in SECRET_KEY_NAMES {
        out = redact_assignment_key(&out, key);
        out = redact_colon_key(&out, key);
    }
    out = redact_bearer_tokens(&out);
    out = redact_prefixed_token(&out, "AIza", 20, is_google_key_char);
    out = redact_prefixed_token(&out, "ya29.", 10, is_oauth_token_char);
    out = redact_prefixed_token(&out, "1//", 10, is_oauth_token_char);
    redact_jwt_tokens(&out)
}

/// Convenience wrapper around [`redact_secrets`] for owned [`String`] slices.
#[must_use]
pub fn redact_secrets_owned(text: &str, secrets: &[String]) -> String {
    let borrowed: Vec<&str> = secrets.iter().map(String::as_str).collect();
    redact_secrets(text, &borrowed)
}

/// Extracts password-bearing fragments from a PostgreSQL connection string so callers
/// can feed them into [`redact_secrets`] alongside other secrets.
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
            if !userinfo.is_empty() {
                secrets.push(userinfo.to_owned());
            }
            if let Some(colon_pos) = userinfo.find(':') {
                let raw_pass = &userinfo[colon_pos + 1..];
                push_secret_variants(&mut secrets, raw_pass);
            }
        }
    }

    for val in extract_assignment_values(trimmed, "password") {
        push_secret_variants(&mut secrets, &val);
    }

    if !secrets.is_empty() {
        secrets.push(trimmed.to_owned());
    }
    secrets
}

fn push_secret_variants(dst: &mut Vec<String>, raw: &str) {
    if raw.is_empty() {
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

fn is_word_boundary_left(bytes: &[u8], pos: usize) -> bool {
    if pos == 0 {
        return true;
    }
    let prev = bytes[pos - 1];
    !(prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'-')
}

fn scan_quoted_value(rest: &str, quote: u8) -> (usize, String) {
    let bytes = rest.as_bytes();
    let mut i = 1;
    let mut inner = String::new();
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            inner.push(bytes[i + 1] as char);
            i += 2;
            continue;
        }
        if bytes[i] == quote {
            if quote == b'\'' && i + 1 < bytes.len() && bytes[i + 1] == b'\'' {
                inner.push('\'');
                i += 2;
                continue;
            }
            return (i + 1, inner);
        }
        let ch = rest[i..].chars().next().unwrap_or('\0');
        inner.push(ch);
        i += ch.len_utf8();
    }
    (rest.len(), inner)
}

fn scan_unquoted_value(rest: &str) -> usize {
    rest.find(|c: char| {
        c.is_whitespace() || matches!(c, '&' | ';' | ',' | '"' | '\'' | ')' | '}' | ']')
    })
    .unwrap_or(rest.len())
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
            Some(&b'\'') => scan_quoted_value(val_rest, b'\'').0,
            Some(&b'"') => scan_quoted_value(val_rest, b'"').0,
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
            Some(&b'\'') => scan_quoted_value(val_rest, b'\''),
            Some(&b'"') => scan_quoted_value(val_rest, b'"'),
            _ => {
                let len = scan_unquoted_value(val_rest);
                (len, val_rest[..len].to_owned())
            }
        };
        if !val.is_empty() {
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
                scan_quoted_value(val_rest, b'"').0
            }
            Some(&b'\'') => {
                result.push('\'');
                result.push_str(REDACTED_PLACEHOLDER);
                result.push('\'');
                scan_quoted_value(val_rest, b'\'').0
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
        let token_len = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | ',' | ';' | ')' | '}'))
            .unwrap_or(rest.len());
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
    use super::*;

    #[test]
    fn redacts_extended_oauth_header_json_and_escaped_password_patterns() {
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
        assert!(
            cleaned.contains("127.0.0.1:55432/db"),
            "non-secret host/db info should be preserved: {cleaned}"
        );
    }
}
