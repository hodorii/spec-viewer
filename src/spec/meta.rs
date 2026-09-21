use std::collections::BTreeMap;

use serde_json::Value;

/// Kind of document tracked within a spec directory.
///
/// Declaration order matters (design.md 2.2: "순서 = 선언 순서"); the five
/// named variants correspond to the `approvals` keys recognized in
/// `spec.json` (`requirements`/`bugfix`/`bizProcess`/`design`/`tasks`).
/// `Research` and `Other` are not populated by `parse_meta` — they exist for
/// later document-tree assembly tasks.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DocKind {
    Requirements,
    Bugfix,
    BizProcess,
    Design,
    Tasks,
    Research,
    Other(String),
}

/// Approval state for a single document, as recorded in `spec.json`'s
/// `approvals` object: `{ "generated": bool, "approved": bool }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Approval {
    pub generated: bool,
    pub approved: bool,
}

/// Normalized view of a `spec.json` file's contents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecMeta {
    pub name: String,
    pub phase: String,
    pub approvals: BTreeMap<DocKind, Approval>,
    /// Raw `updated_at` string (requirement 2.9's "최근 갱신" sort key),
    /// e.g. `"2026-01-02T00:00:00Z"`. Kept as the literal ISO-8601 string
    /// rather than parsed into a timestamp type -- ISO-8601 sorts
    /// lexicographically, which is all `spec::sort::sort_specs` needs.
    /// `None` when the field is absent or not a string.
    pub updated_at: Option<String>,
}

/// Failure parsing a `spec.json` file's contents.
#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum MetaError {
    #[error("invalid JSON: {0}")]
    InvalidJson(String),
    #[error("missing required name field (\"feature_name\" or \"name\")")]
    MissingName,
}

/// Parse the raw contents of a `spec.json` file into a [`SpecMeta`].
///
/// Lenient by design (requirement 3.4's "누락 항목 → 정상 동작 유지"): only two
/// failure modes produce `Err`:
/// 1. `json` is not valid JSON at all.
/// 2. Valid JSON, but neither `"feature_name"` nor `"name"` is present as a
///    string.
///
/// Everything else — missing `phase`, missing/partial `approvals`, malformed
/// per-entry `generated`/`approved` fields, unrecognized approval keys — is
/// tolerated and degrades to a lenient default rather than failing the whole
/// parse.
pub fn parse_meta(json: &str) -> Result<SpecMeta, MetaError> {
    let value: Value =
        serde_json::from_str(json).map_err(|e| MetaError::InvalidJson(e.to_string()))?;

    // Prefer "feature_name" over "name" per requirement 3.3 (schema variance).
    let name = value
        .get("feature_name")
        .and_then(Value::as_str)
        .or_else(|| value.get("name").and_then(Value::as_str))
        .ok_or(MetaError::MissingName)?
        .to_string();

    let phase = value
        .get("phase")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let updated_at = value
        .get("updated_at")
        .and_then(Value::as_str)
        .map(str::to_string);

    let mut approvals = BTreeMap::new();
    if let Some(obj) = value.get("approvals").and_then(Value::as_object) {
        for (key, entry) in obj {
            let kind = match key.as_str() {
                "requirements" => DocKind::Requirements,
                "bugfix" => DocKind::Bugfix,
                "bizProcess" => DocKind::BizProcess,
                "design" => DocKind::Design,
                "tasks" => DocKind::Tasks,
                // Unrecognized keys are ignored silently, not mapped to
                // Other/Research (those are reserved for a later task's
                // document-tree assembly).
                _ => continue,
            };

            // Missing or wrong-typed booleans default to `false` rather than
            // failing the whole parse (requirement 3.4 tolerance).
            let generated = entry
                .get("generated")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let approved = entry
                .get("approved")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            approvals.insert(
                kind,
                Approval {
                    generated,
                    approved,
                },
            );
        }
    }

    Ok(SpecMeta {
        name,
        phase,
        approvals,
        updated_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_BILLING: &str =
        include_str!("../../tests/fixtures/kiro/specs/sample-billing/spec.json");
    const SAMPLE_SIGNUP: &str =
        include_str!("../../tests/fixtures/kiro/specs/sample-signup/spec.json");
    const BROKEN_JSON: &str = include_str!("../../tests/fixtures/kiro/specs/broken-json/spec.json");
    const NO_APPROVALS: &str =
        include_str!("../../tests/fixtures/kiro/specs/no-approvals/spec.json");
    const BUGFIX_ONLY: &str =
        include_str!("../../tests/fixtures/kiro/specs/bugfix-only/spec.json");

    #[test]
    fn sample_billing_uses_feature_name_and_has_full_approvals() {
        let meta = parse_meta(SAMPLE_BILLING).expect("should parse");

        assert_eq!(meta.name, "sample-billing");
        assert_eq!(meta.phase, "design");
        assert_eq!(meta.updated_at.as_deref(), Some("2026-02-02T00:00:00Z"));
        assert_eq!(meta.approvals.len(), 4);
        assert_eq!(
            meta.approvals[&DocKind::Requirements],
            Approval {
                generated: true,
                approved: true
            }
        );
        assert_eq!(
            meta.approvals[&DocKind::BizProcess],
            Approval {
                generated: true,
                approved: true
            }
        );
        assert_eq!(
            meta.approvals[&DocKind::Design],
            Approval {
                generated: true,
                approved: false
            }
        );
        assert_eq!(
            meta.approvals[&DocKind::Tasks],
            Approval {
                generated: false,
                approved: false
            }
        );
    }

    #[test]
    fn sample_signup_uses_name_key() {
        let meta = parse_meta(SAMPLE_SIGNUP).expect("should parse");

        assert_eq!(meta.name, "sample-signup");
        assert_eq!(meta.phase, "implementation");
        assert_eq!(
            meta.approvals[&DocKind::Requirements],
            Approval {
                generated: true,
                approved: true
            }
        );
        assert_eq!(
            meta.approvals[&DocKind::BizProcess],
            Approval {
                generated: true,
                approved: false
            }
        );
        assert_eq!(
            meta.approvals[&DocKind::Design],
            Approval {
                generated: true,
                approved: false
            }
        );
        // No "tasks" key in this fixture.
        assert!(!meta.approvals.contains_key(&DocKind::Tasks));
    }

    #[test]
    fn broken_json_is_an_error() {
        let result = parse_meta(BROKEN_JSON);
        assert!(matches!(result, Err(MetaError::InvalidJson(_))));
    }

    #[test]
    fn no_approvals_key_parses_with_empty_map() {
        let meta = parse_meta(NO_APPROVALS).expect("should parse");

        assert_eq!(meta.name, "no-approvals");
        assert_eq!(meta.phase, "discovery");
        assert!(meta.approvals.is_empty());
        assert_eq!(meta.updated_at, None, "no-approvals fixture has no updated_at field");
    }

    #[test]
    fn bugfix_key_maps_to_bugfix_dockind() {
        let meta = parse_meta(BUGFIX_ONLY).expect("should parse");

        assert_eq!(meta.name, "bugfix-only");
        assert_eq!(
            meta.approvals[&DocKind::Bugfix],
            Approval {
                generated: true,
                approved: true
            }
        );
    }

    #[test]
    fn missing_name_is_an_error() {
        let result = parse_meta(r#"{"phase": "design"}"#);
        assert!(matches!(result, Err(MetaError::MissingName)));
    }

    #[test]
    fn malformed_approval_entry_defaults_to_false_leniently() {
        // "generated" is a string instead of a bool, and "approved" is
        // absent entirely. Per the lenient-parsing rules, both fields
        // default to `false` rather than failing the whole parse.
        let json = r#"{"feature_name": "x", "approvals": {"design": {"generated": "not-a-bool"}}}"#;

        let meta = parse_meta(json).expect("should parse leniently");

        assert_eq!(
            meta.approvals[&DocKind::Design],
            Approval {
                generated: false,
                approved: false
            }
        );
    }
}
