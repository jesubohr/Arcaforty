//! Parsers for `.env` and CSV files. They only build entries; the caller reviews them
//! before adding anything to the vault.

use crate::entry::{Entry, Kind, limits};
use crate::error::{Error, Result};

pub const MAX_IMPORT_BYTES: usize = 5 * 1024 * 1024;

#[derive(Default)]
pub struct Parsed {
    pub entries: Vec<Entry>,
    /// Lines or rows that were skipped, with the reason.
    pub warnings: Vec<String>,
}

impl Parsed {
    fn push(&mut self, place: String, entry: Result<Entry>) {
        match entry {
            Ok(e) => self.entries.push(e),
            Err(err) => self.warnings.push(format!("{place}: {err}")),
        }
    }
}

fn check_size(text: &str) -> Result<&str> {
    if text.len() > MAX_IMPORT_BYTES {
        return Err(Error::Limit(
            "The file is too large to import (5 MiB maximum).",
        ));
    }
    Ok(text.strip_prefix('\u{feff}').unwrap_or(text))
}

/// One entry per `KEY=value` line. Supports `export`, comments, and quoted values on one line.
pub fn parse_env(text: &str, now: i64) -> Result<Parsed> {
    let text = check_size(text)?;
    let mut parsed = Parsed::default();
    for (i, raw) in text.lines().enumerate() {
        let place = format!("Line {}", i + 1);
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").map_or(line, str::trim_start);
        let Some((key, rest)) = line.split_once('=') else {
            parsed.warnings.push(format!("{place}: no '=' found."));
            continue;
        };
        let key = key.trim();
        if !valid_env_key(key) {
            parsed
                .warnings
                .push(format!("{place}: invalid variable name."));
            continue;
        }
        let value = match env_value(rest.trim_start()) {
            Some(v) if !v.is_empty() => v,
            Some(_) => {
                parsed.warnings.push(format!("{place}: empty value."));
                continue;
            }
            None => {
                parsed
                    .warnings
                    .push(format!("{place}: unterminated quote."));
                continue;
            }
        };
        let upper = key.to_ascii_uppercase();
        let kind = if upper.contains("PRIVATE_KEY") {
            Kind::PrivateKey
        } else if upper.contains("PASS") {
            Kind::Password
        } else {
            Kind::ApiKey
        };
        parsed.push(
            place,
            Entry::new(key, kind, &value, "Imported from .env", now),
        );
    }
    if parsed.entries.len() > limits::ENTRIES {
        return Err(Error::Limit("The file has too many entries."));
    }
    Ok(parsed)
}

fn valid_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// None means an unterminated quote.
fn env_value(rest: &str) -> Option<String> {
    let mut chars = rest.chars();
    match chars.next() {
        Some('"') => {
            let mut out = String::new();
            while let Some(c) = chars.next() {
                match c {
                    '"' => return Some(out),
                    '\\' => match chars.next()? {
                        'n' => out.push('\n'),
                        't' => out.push('\t'),
                        other => out.push(other),
                    },
                    _ => out.push(c),
                }
            }
            None
        }
        Some('\'') => {
            let body = &rest[1..];
            body.find('\'').map(|end| body[..end].to_string())
        }
        _ => {
            // Unquoted: a comment starts at whitespace followed by '#'.
            let end = rest
                .match_indices('#')
                .find(|(i, _)| rest[..*i].ends_with(char::is_whitespace))
                .map_or(rest.len(), |(i, _)| i);
            Some(rest[..end].trim_end().to_string())
        }
    }
}

/// CSV with a header row. Understands common password manager exports.
pub fn parse_csv(text: &str, now: i64) -> Result<Parsed> {
    let text = check_size(text)?;
    let rows = csv_rows(text)?;
    let Some((header, body)) = rows.split_first() else {
        return Err(Error::Format("The CSV file is empty."));
    };
    let header: Vec<String> = header
        .iter()
        .map(|h| h.trim().to_ascii_lowercase())
        .collect();
    let find = |names: &[&str]| {
        names
            .iter()
            .find_map(|n| header.iter().position(|h| h == n))
    };
    let platform = find(&["name", "title", "platform", "site", "service", "account"]);
    let url = find(&["url", "login_uri", "website", "uri"]);
    let (Some(value), true) = (
        find(&[
            "password",
            "login_password",
            "secret",
            "value",
            "key",
            "token",
        ]),
        platform.is_some() || url.is_some(),
    ) else {
        return Err(Error::Format(
            "The CSV needs a header row with a name and a password column.",
        ));
    };
    let user = find(&["username", "login_username", "user"]);
    let notes = find(&["notes", "note", "comment", "comments"]);
    let kind = find(&["type", "kind"]);

    let mut parsed = Parsed::default();
    for (i, row) in body.iter().enumerate() {
        let place = format!("Row {}", i + 2);
        let raw = |col: Option<usize>| col.and_then(|c| row.get(c)).map_or("", String::as_str);
        let cell = |col: Option<usize>| raw(col).trim();
        if row.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        let name = match cell(platform) {
            "" => cell(url),
            n => n,
        };
        let mut extra = String::new();
        for (label, text) in [("User", cell(user)), ("URL", cell(url))] {
            if !text.is_empty() {
                extra.push_str(&format!("{label}: {text}\n"));
            }
        }
        extra.push_str(cell(notes));
        let k = Kind::parse(cell(kind)).unwrap_or(Kind::Password);
        // The secret keeps its spaces: they can be part of a password.
        parsed.push(place, Entry::new(name, k, raw(Some(value)), &extra, now));
    }
    Ok(parsed)
}

/// RFC 4180 style: quoted fields, doubled quotes, newlines inside quotes, CRLF or LF.
fn csv_rows(text: &str) -> Result<Vec<Vec<String>>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            ('"', true) => quoted = false,
            ('"', false) if field.is_empty() => quoted = true,
            (',', false) => row.push(std::mem::take(&mut field)),
            ('\r', false) => {}
            ('\n', false) => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
                if rows.len() > limits::ENTRIES + 1 {
                    return Err(Error::Limit("The file has too many rows."));
                }
            }
            _ => field.push(c),
        }
    }
    if quoted {
        return Err(Error::Format("The CSV has an unterminated quote."));
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    Ok(rows)
}
