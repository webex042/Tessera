//! Strict parsing of the extraction output (plan §16.1 schema, §8.3 step 1).
//!
//! The model is constrained by a grammar, but code validates again and accepts **only** the
//! schema: one JSON object (no code fences, no prose around it), no unknown fields, enums from the
//! fixed lists, length limits counted in characters, no control characters, no empty strings.
//! Any violation rejects the whole output — it is never patched into something valid-looking.

use serde::Deserialize;

/// Bumped when the stored fact layout or these rules change; stored with every fact.
pub const SCHEMA_VER: u32 = 1;

pub const KINDS: [&str; 6] = ["task", "commitment", "decision", "event", "question", "note"];
pub const CONFIDENCE: [&str; 3] = ["low", "medium", "high"];

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawFact {
    pub kind: String,
    pub text: String,
    pub owner: String,
    #[serde(default)]
    pub counterparty: Option<String>,
    #[serde(default)]
    pub due_text: Option<String>,
    pub evidence: String,
    pub confidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawPerson {
    pub name: String,
    #[serde(default)]
    pub handle: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Output {
    pub summary: String,
    pub facts: Vec<RawFact>,
    pub people: Vec<RawPerson>,
}

impl std::fmt::Display for RawFact {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<fact kind={}>", self.kind) // never the content
    }
}

/// Content-free reason; `field` names a schema field, never a value.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, serde::Serialize)]
pub enum SchemaError {
    #[error("not a single JSON object")]
    NotJson,
    #[error("unknown or missing field")]
    Shape,
    #[error("{0}: not an allowed value")]
    Enum(&'static str),
    #[error("{0}: too long")]
    TooLong(&'static str),
    #[error("{0}: empty")]
    Empty(&'static str),
    #[error("{0}: control characters")]
    Control(&'static str),
    #[error("too many {0}")]
    TooMany(&'static str),
}

fn check(name: &'static str, s: &str, max: usize, allow_newline: bool) -> Result<(), SchemaError> {
    if s.trim().is_empty() {
        return Err(SchemaError::Empty(name));
    }
    if s.chars().count() > max {
        return Err(SchemaError::TooLong(name));
    }
    if s.chars().any(|c| c.is_control() && !(allow_newline && c == '\n')) {
        return Err(SchemaError::Control(name));
    }
    Ok(())
}

fn opt(name: &'static str, s: &Option<String>, max: usize) -> Result<(), SchemaError> {
    match s {
        Some(v) => check(name, v, max, false),
        None => Ok(()),
    }
}

pub fn parse(raw: &str) -> Result<Output, SchemaError> {
    let t = raw.trim();
    if !(t.starts_with('{') && t.ends_with('}')) {
        return Err(SchemaError::NotJson);
    }
    // Two passes: first "is it JSON at all", then the typed shape (unknown fields rejected).
    let v: serde_json::Value = serde_json::from_str(t).map_err(|_| SchemaError::NotJson)?;
    let out: Output = serde_json::from_value(v).map_err(|_| SchemaError::Shape)?;
    if out.summary.chars().count() > 280 {
        return Err(SchemaError::TooLong("summary"));
    }
    if out.summary.chars().any(|c| c.is_control() && c != '\n') {
        return Err(SchemaError::Control("summary"));
    }
    if out.facts.len() > 12 {
        return Err(SchemaError::TooMany("facts"));
    }
    if out.people.len() > 10 {
        return Err(SchemaError::TooMany("people"));
    }
    for f in &out.facts {
        if !KINDS.contains(&f.kind.as_str()) {
            return Err(SchemaError::Enum("kind"));
        }
        if !CONFIDENCE.contains(&f.confidence.as_str()) {
            return Err(SchemaError::Enum("confidence"));
        }
        check("text", &f.text, 200, false)?;
        check("owner", &f.owner, 80, false)?;
        check("evidence", &f.evidence, 200, true)?;
        opt("counterparty", &f.counterparty, 80)?;
        opt("due_text", &f.due_text, 60)?;
    }
    for p in &out.people {
        check("name", &p.name, 80, false)?;
        opt("handle", &p.handle, 120)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"{"summary":"Ravi asked for the sheet.","facts":[{"kind":"commitment","text":"Send Ravi the revised sheet","owner":"me","counterparty":"Ravi Iyer","due_text":"by Friday","evidence":"I'll send the revised sheet by Friday","confidence":"high"}],"people":[{"name":"Ravi Iyer","handle":"ravi.iyer@northwind.example"}]}"#;

    #[test]
    fn valid_output_parses() {
        let o = parse(GOOD).unwrap();
        assert_eq!(o.facts.len(), 1);
        assert_eq!(
            parse(r#"{"summary":"","facts":[],"people":[]}"#).unwrap().facts.len(),
            0
        );
    }

    #[test]
    fn malformed_and_wrapped_json_is_rejected() {
        for bad in [
            "",
            "Sure! Here is the JSON:",
            &format!("```json\n{GOOD}\n```"),
            &format!("{GOOD} trailing"),
            r#"{"summary": "x", "facts": [}"#,
            "[]",
            "null",
        ] {
            assert_eq!(parse(bad).unwrap_err(), SchemaError::NotJson, "{bad:?}");
        }
    }

    #[test]
    fn shape_violations_are_rejected_not_coerced() {
        let cases = [
            (
                r#"{"summary":"x","facts":[],"people":[],"action":"send_email"}"#,
                SchemaError::Shape,
            ),
            (r#"{"summary":"x","facts":[]}"#, SchemaError::Shape),
            (r#"{"summary":1,"facts":[],"people":[]}"#, SchemaError::Shape),
            (
                r#"{"summary":"x","facts":[{"kind":"task","text":"t","owner":"me","evidence":"e e e","confidence":"high","url":"http://x"}],"people":[]}"#,
                SchemaError::Shape,
            ),
            (
                r#"{"summary":"x","facts":[{"kind":"reminder","text":"t","owner":"me","evidence":"e","confidence":"high"}],"people":[]}"#,
                SchemaError::Enum("kind"),
            ),
            (
                r#"{"summary":"x","facts":[{"kind":"task","text":"t","owner":"me","evidence":"e","confidence":"certain"}],"people":[]}"#,
                SchemaError::Enum("confidence"),
            ),
            (
                r#"{"summary":"x","facts":[{"kind":"task","text":" ","owner":"me","evidence":"e","confidence":"low"}],"people":[]}"#,
                SchemaError::Empty("text"),
            ),
            (
                r#"{"summary":"x","facts":[{"kind":"task","text":"t\u0007","owner":"me","evidence":"e","confidence":"low"}],"people":[]}"#,
                SchemaError::Control("text"),
            ),
        ];
        for (json, want) in cases {
            assert_eq!(parse(json).unwrap_err(), want, "{json}");
        }
        let long = format!(r#"{{"summary":"{}","facts":[],"people":[]}}"#, "x".repeat(281));
        assert_eq!(parse(&long).unwrap_err(), SchemaError::TooLong("summary"));
        // 200 characters of a 3-byte script is fine; 201 is not (limits are characters).
        let hindi = "क".repeat(200);
        let ok = format!(
            r#"{{"summary":"","facts":[{{"kind":"note","text":"{hindi}","owner":"me","evidence":"a b c d","confidence":"low"}}],"people":[]}}"#
        );
        assert!(parse(&ok).is_ok());
        let f = r#"{"kind":"note","text":"t","owner":"me","evidence":"e","confidence":"low"}"#;
        let many = format!(r#"{{"summary":"","facts":[{}],"people":[]}}"#, vec![f; 13].join(","));
        assert_eq!(parse(&many).unwrap_err(), SchemaError::TooMany("facts"));
    }

    #[test]
    fn display_never_shows_content() {
        let o = parse(GOOD).unwrap();
        assert_eq!(o.facts[0].to_string(), "<fact kind=commitment>");
    }
}
