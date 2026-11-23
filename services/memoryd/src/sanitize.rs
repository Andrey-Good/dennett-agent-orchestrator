//! Credential sanitization for PostgreSQL connection strings, CLI arguments, and error messages.

/// Replacement placeholder inserted wherever a secret value is stripped.
pub const REDACTED: &str = "[REDACTED]";

/// Redacts credentials embedded in `pg_url` and generic URI/key-value/query password patterns
/// from `raw_error`.
#[must_use]
pub fn sanitize_pg_error(raw_error: &str, pg_url: Option<&str>) -> String {
    let mut out = raw_error.to_owned();

    if let Some(url) = pg_url {
        let mut secrets = extract_pg_secrets(url);
        secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
        for secret in secrets {
            if !secret.is_empty() {
                out = out.replace(&secret, REDACTED);
            }
        }
    }

    out = redact_uri_userinfo(&out);
    redact_assignment_key(&out, "password")
}

/// Sanitizes an unrecognized CLI argument before echoing it in an error message.
///
/// Any `--flag=value` argument has its value stripped after `=`, and the entire string is
/// also passed through [`sanitize_pg_error`] so bare connection URIs never leak.
#[must_use]
pub fn sanitize_cli_arg(arg: &str) -> String {
    let truncated = match arg.split_once('=') {
        Some((flag, _value)) => format!("{flag}={REDACTED}"),
        None => arg.to_owned(),
    };
    sanitize_pg_error(&truncated, None)
}

fn extract_pg_secrets(conn_str: &str) -> Vec<String> {
    let mut secrets = Vec::new();
    let trimmed = conn_str.trim();
    if trimmed.is_empty() {
        return secrets;
    }

    // 1. URI form: scheme://userinfo@host... (supports special chars like `;,)]}>/'` in password).
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
                push_secret_and_decoded(&mut secrets, raw_pass);
            }
        }
    }

    // 2. libpq key-value and URI query form (`password=`, `password = `, `?password=`, `&password=`).
    for raw_val in extract_assignment_values(trimmed, "password") {
        push_secret_and_decoded(&mut secrets, &raw_val);
    }

    if !secrets.is_empty() {
        secrets.push(trimmed.to_owned());
    }
    secrets
}

fn push_secret_and_decoded(dst: &mut Vec<String>, raw: &str) {
    if raw.is_empty() {
        return;
    }
    dst.push(raw.to_owned());
    let decoded = percent_decode(raw);
    if decoded != raw && !decoded.is_empty() {
        dst.push(decoded);
    }
    let unescaped = unescape_single_quoted(raw);
    if unescaped != raw && !unescaped.is_empty() {
        dst.push(unescaped);
    }
}

/// Finds the `@` separating URI `userinfo` from `host` inside a whitespace-free URI span.
///
/// Picks the rightmost `@` followed by a plausible host character where either no `/`, `?`, `#`
/// precedes `@`, or a `:` appears before `@` (allowing passwords containing `/` or punctuation
/// such as `;,)]}>'`).
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
            result.push_str(REDACTED);
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
    !(prev.is_ascii_alphanumeric() || prev == b'_')
}

fn scan_quoted_value(rest: &str, quote: u8) -> (usize, String) {
    let bytes = rest.as_bytes();
    let mut i = 1; // skip opening quote
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
        result.push_str(REDACTED);

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

fn unescape_single_quoted(input: &str) -> String {
    input.replace("\\'", "'").replace("''", "'")
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
    fn redacts_uri_and_decoded_password() {
        let url =
            "postgres://dennett_memory_role:s3cr%40tPass!@127.0.0.1:55432/dennett_memory_test";
        let raw = "failed to connect to postgres://dennett_memory_role:s3cr%40tPass!@127.0.0.1:55432/dennett_memory_test with s3cr@tPass!";
        let cleaned = sanitize_pg_error(raw, Some(url));
        assert!(
            !cleaned.contains("s3cr%40tPass!"),
            "encoded pass leaked: {cleaned}"
        );
        assert!(
            !cleaned.contains("s3cr@tPass!"),
            "decoded pass leaked: {cleaned}"
        );
        assert!(
            cleaned.contains(REDACTED),
            "redaction marker missing: {cleaned}"
        );
    }

    #[test]
    fn redacts_uri_password_with_punctuation_and_slashes() {
        let raw =
            "connect error postgres://u:p;a,r)a]m}s>'/SECRET@127.0.0.1:55432/dennett_memory_test";
        let cleaned = sanitize_pg_error(raw, None);
        assert!(
            !cleaned.contains("SECRET"),
            "punctuated URI password leaked: {cleaned}"
        );
        assert!(
            cleaned.contains("127.0.0.1:55432/dennett_memory_test"),
            "host/db context should stay readable: {cleaned}"
        );
    }

    #[test]
    fn redacts_spaced_kv_escaped_quotes_and_query_passwords() {
        let raw = "libpq password = spaced_secret_1; quoted password='a\\'ESCAPED_SECRET_2' \
                   uri postgres://127.0.0.1/db?user=u&password=query_secret_3&sslmode=disable \
                   alt ?password=query_secret_4";
        let cleaned = sanitize_pg_error(raw, None);
        for secret in [
            "spaced_secret_1",
            "ESCAPED_SECRET_2",
            "query_secret_3",
            "query_secret_4",
        ] {
            assert!(
                !cleaned.contains(secret),
                "secret `{secret}` leaked: {cleaned}"
            );
        }
    }

    #[test]
    fn sanitize_cli_arg_strips_flag_value_and_bare_uri_secret() {
        let flag_err = sanitize_cli_arg("--pg-url=postgres://u:CLI_SECRET_9@127.0.0.1/db");
        assert!(
            !flag_err.contains("CLI_SECRET_9"),
            "flag secret leaked: {flag_err}"
        );
        assert!(
            flag_err.starts_with("--pg-url="),
            "flag name should remain: {flag_err}"
        );

        let bare_err = sanitize_cli_arg("postgres://u:BARE_SECRET_8@127.0.0.1/db");
        assert!(
            !bare_err.contains("BARE_SECRET_8"),
            "bare URI secret leaked: {bare_err}"
        );
    }

    proptest! {
        #[test]
        fn prop_sanitize_pg_error_never_leaks_uri_password(
            user in "[a-z][a-z0-9_]{1,12}",
            pass in "[A-Za-z0-9!#$%^&*()_+\\-=;,.>'/]{8,32}",
        ) {
            let url = format!("postgres://{user}:{pass}@127.0.0.1:55432/dennett_memory_test");
            let msg = format!("connection failed for {url} (retry)");
            let cleaned_with_url = sanitize_pg_error(&msg, Some(&url));
            let cleaned_without_url = sanitize_pg_error(&msg, None);
            prop_assert!(!cleaned_with_url.contains(&pass), "leaked with url: {cleaned_with_url}");
            prop_assert!(!cleaned_without_url.contains(&pass), "leaked without url: {cleaned_without_url}");
            prop_assert!(cleaned_without_url.contains("127.0.0.1:55432/dennett_memory_test"));
        }

        #[test]
        fn prop_sanitize_pg_error_never_leaks_kv_or_query_password(
            pass in "[A-Za-z0-9_!#%-]{8,28}",
            spaces in "[ ]{0,3}",
        ) {
            let raw = format!(
                "libpq password{spaces}={spaces}{pass} query postgres://127.0.0.1/db?password={pass}&sslmode=disable"
            );
            let cleaned = sanitize_pg_error(&raw, None);
            prop_assert!(!cleaned.contains(&pass), "kv/query password leaked: {cleaned}");
        }

        #[test]
        fn prop_sanitize_cli_arg_never_leaks_flag_value(
            flag in "[a-z][a-z0-9-]{1,16}",
            secret in "[A-Za-z0-9_!@#:/-]{8,32}",
        ) {
            let arg = format!("--{flag}={secret}");
            let cleaned = sanitize_cli_arg(&arg);
            prop_assert!(!cleaned.contains(&secret), "CLI flag secret leaked: {cleaned}");
        }

        #[test]
        fn prop_sanitize_pg_error_is_idempotent_on_arbitrary_unicode(raw in any::<String>()) {
            let once = sanitize_pg_error(&raw, None);
            let twice = sanitize_pg_error(&once, None);
            prop_assert_eq!(once, twice);
        }
    }
}
