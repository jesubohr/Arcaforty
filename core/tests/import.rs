use arcaforty_core::Kind;
use arcaforty_core::import::{MAX_IMPORT_BYTES, parse_csv, parse_env};

#[test]
fn env_basic() {
    let text = "# comment\n\nOPENAI_API_KEY=sk-123\nexport DB_PASSWORD = \"p@ss word\" # note\nTOKEN='single # kept'\nPLAIN=abc # trailing\nHASH=ab#cd\n";
    let p = parse_env(text, 0).unwrap();
    let get = |n: &str| p.entries.iter().find(|e| e.platform == n).unwrap();
    assert_eq!(p.entries.len(), 5);
    assert_eq!(get("OPENAI_API_KEY").value, "sk-123");
    assert_eq!(get("OPENAI_API_KEY").kind, Kind::ApiKey);
    assert_eq!(get("DB_PASSWORD").value, "p@ss word");
    assert_eq!(get("DB_PASSWORD").kind, Kind::Password);
    assert_eq!(get("TOKEN").value, "single # kept");
    assert_eq!(get("PLAIN").value, "abc");
    assert_eq!(get("HASH").value, "ab#cd");
    assert!(p.warnings.is_empty());
}

#[test]
fn env_escapes_and_bad_lines() {
    let text =
        "A=\"line1\\nline2 \\\"q\\\"\"\nnonsense\n1BAD=x\nEMPTY=\nOPEN=\"unterminated\nBOM_OK=1\n";
    let p = parse_env(text, 0).unwrap();
    assert_eq!(p.entries[0].value, "line1\nline2 \"q\"");
    assert_eq!(p.entries.len(), 2);
    assert_eq!(p.warnings.len(), 4);
    assert!(p.warnings[0].starts_with("Line 2"));
}

#[test]
fn env_strips_bom() {
    let p = parse_env("\u{feff}KEY=value", 0).unwrap();
    assert_eq!(p.entries[0].platform, "KEY");
}

#[test]
fn csv_chrome_style() {
    let text = "name,url,username,password,note\r\nGitHub,https://github.com,me,\"pa,ss\"\"word\",\"two\nlines\"\r\n,,,,\r\nNoPass,https://x.io,u,,\r\n";
    let p = parse_csv(text, 0).unwrap();
    assert_eq!(p.entries.len(), 1);
    let e = &p.entries[0];
    assert_eq!(
        (e.platform.as_str(), e.value.as_str()),
        ("GitHub", "pa,ss\"word")
    );
    assert_eq!(e.kind, Kind::Password);
    assert_eq!(e.notes, "User: me\nURL: https://github.com\ntwo\nlines");
    assert_eq!(p.warnings.len(), 1);
    assert!(p.warnings[0].starts_with("Row 4"));
}

#[test]
fn csv_kind_and_alt_headers() {
    let text = "Title,Password,Type\nLedger,abandon ability,Seed phrase\nSite,pw,Login\n";
    let p = parse_csv(text, 0).unwrap();
    assert_eq!(p.entries[0].kind, Kind::SeedPhrase);
    assert_eq!(p.entries[1].kind, Kind::Password);
}

#[test]
fn csv_keeps_spaces_in_secrets_and_trims_metadata() {
    let text = "name,url,username,password\n  Bank  ,,  me  ,\"  pa ss  \"\nPlain,,,  lead\n";
    let p = parse_csv(text, 0).unwrap();
    assert_eq!(p.entries[0].platform, "Bank");
    assert_eq!(p.entries[0].notes, "User: me");
    assert_eq!(p.entries[0].value, "  pa ss  ");
    assert_eq!(p.entries[1].value, "  lead");
}

#[test]
fn csv_quoted_whitespace_password_roundtrips() {
    let password = " \t spaced \"quoted\", out \n";
    let escaped = password.replace('"', "\"\"");
    let text = format!("name,password\nSite,\"{escaped}\"\n");
    let p = parse_csv(&text, 0).unwrap();
    assert_eq!(p.entries[0].value, password);
}

#[test]
fn csv_rejects_bad_input() {
    assert!(parse_csv("", 0).is_err());
    assert!(parse_csv("a,b\n1,2\n", 0).is_err());
    assert!(parse_csv("name,password\n\"open,x\n", 0).is_err());
    let big = "x".repeat(MAX_IMPORT_BYTES + 1);
    assert!(parse_csv(&big, 0).is_err());
    assert!(parse_env(&big, 0).is_err());
}
