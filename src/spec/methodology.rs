use serde::Deserialize;

/// Which file is a spec's task list and where each metadata tag leads.
/// Declarative only: parsing and navigation live elsewhere and read these rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodologySettings {
    pub task_list_file: String,
    pub tags: Vec<TagRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TagRule {
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    /// `None` means the tag is plain text with nowhere to jump.
    #[serde(default)]
    pub link: Option<LinkRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LinkRule {
    /// Candidate target files, first existing one wins.
    pub targets: Vec<LinkTarget>,
    pub locate: LocateBy,
    pub values: ValueSplit,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkTarget {
    SameFile,
    File(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocateBy {
    LeadingNumber,
    TitleText,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueSplit {
    CommaList,
    Whole,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SettingsError {
    #[error("invalid methodology settings JSON: {0}")]
    InvalidJson(String),
}

/// Shape of the user's override file; every field is optional so a partial file
/// keeps the defaults for whatever it leaves out.
#[derive(Debug, Deserialize)]
struct SettingsOverrides {
    task_list_file: Option<String>,
    tags: Option<Vec<TagRule>>,
}

impl MethodologySettings {
    pub fn kiro_default() -> Self {
        let numbered_in_requirements = || LinkRule {
            targets: vec![
                LinkTarget::File("requirements.md".into()),
                LinkTarget::File("bugfix.md".into()),
            ],
            locate: LocateBy::LeadingNumber,
            values: ValueSplit::CommaList,
        };
        let plain_tag = |name: &str, aliases: &[&str]| TagRule {
            name: name.into(),
            aliases: aliases.iter().map(|alias| (*alias).into()).collect(),
            link: None,
        };
        Self {
            task_list_file: "tasks.md".into(),
            tags: vec![
                TagRule {
                    name: "Requirements".into(),
                    aliases: vec![],
                    link: Some(numbered_in_requirements()),
                },
                TagRule {
                    name: "Boundary".into(),
                    aliases: vec![],
                    link: Some(numbered_in_requirements()),
                },
                TagRule {
                    name: "Depends".into(),
                    aliases: vec![],
                    link: Some(LinkRule {
                        targets: vec![LinkTarget::SameFile],
                        locate: LocateBy::LeadingNumber,
                        values: ValueSplit::CommaList,
                    }),
                },
                TagRule {
                    name: "BizProcess".into(),
                    aliases: vec![],
                    link: Some(LinkRule {
                        targets: vec![LinkTarget::File("biz-process.md".into())],
                        locate: LocateBy::TitleText,
                        values: ValueSplit::Whole,
                    }),
                },
                plain_tag("DoneWhen", &["DONE"]),
                plain_tag("Difficulty", &[]),
                plain_tag("Verify", &[]),
            ],
        }
    }

    /// Layers a user's JSON over the defaults: a listed tag replaces the default
    /// of the same name wholesale, an unseen name is appended, absent fields keep
    /// their defaults.
    pub fn parse_overrides(json: &str) -> Result<Self, SettingsError> {
        let overrides: SettingsOverrides = serde_json::from_str(json)
            .map_err(|error| SettingsError::InvalidJson(error.to_string()))?;
        let mut settings = Self::kiro_default();
        if let Some(task_list_file) = overrides.task_list_file {
            settings.task_list_file = task_list_file;
        }
        for tag in overrides.tags.unwrap_or_default() {
            match settings.tags.iter_mut().find(|existing| existing.name == tag.name) {
                Some(existing) => *existing = tag,
                None => settings.tags.push(tag),
            }
        }
        Ok(settings)
    }

    /// Looks up by canonical name or any alias.
    pub fn rule_for(&self, tag_name: &str) -> Option<&TagRule> {
        self.tags.iter().find(|rule| {
            rule.name == tag_name || rule.aliases.iter().any(|alias| alias == tag_name)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn link_of<'a>(settings: &'a MethodologySettings, name: &str) -> Option<&'a LinkRule> {
        settings.rule_for(name).and_then(|rule| rule.link.as_ref())
    }

    #[test]
    fn default_task_list_file_is_tasks_md() {
        assert_eq!(MethodologySettings::kiro_default().task_list_file, "tasks.md");
    }

    #[test]
    fn default_requirements_and_boundary_point_at_requirements_then_bugfix() {
        let settings = MethodologySettings::kiro_default();
        for name in ["Requirements", "Boundary"] {
            let link = link_of(&settings, name).expect(name);
            assert_eq!(
                link.targets,
                vec![
                    LinkTarget::File("requirements.md".into()),
                    LinkTarget::File("bugfix.md".into())
                ]
            );
            assert_eq!(link.locate, LocateBy::LeadingNumber);
            assert_eq!(link.values, ValueSplit::CommaList);
        }
    }

    #[test]
    fn default_depends_stays_in_the_same_file() {
        let settings = MethodologySettings::kiro_default();
        let link = link_of(&settings, "Depends").expect("Depends");
        assert_eq!(link.targets, vec![LinkTarget::SameFile]);
        assert_eq!(link.locate, LocateBy::LeadingNumber);
        assert_eq!(link.values, ValueSplit::CommaList);
    }

    #[test]
    fn default_bizprocess_locates_title_text_in_biz_process_md() {
        let settings = MethodologySettings::kiro_default();
        let link = link_of(&settings, "BizProcess").expect("BizProcess");
        assert_eq!(link.targets, vec![LinkTarget::File("biz-process.md".into())]);
        assert_eq!(link.locate, LocateBy::TitleText);
        assert_eq!(link.values, ValueSplit::Whole);
    }

    #[test]
    fn default_donewhen_has_done_alias_and_no_link() {
        let settings = MethodologySettings::kiro_default();
        let rule = settings.rule_for("DoneWhen").expect("DoneWhen");
        assert_eq!(rule.aliases, vec!["DONE".to_string()]);
        assert_eq!(rule.link, None);
    }

    #[test]
    fn default_difficulty_and_verify_have_no_link() {
        let settings = MethodologySettings::kiro_default();
        for name in ["Difficulty", "Verify"] {
            assert_eq!(settings.rule_for(name).expect(name).link, None, "{name}");
        }
    }

    #[test]
    fn rule_for_resolves_alias_to_canonical_rule() {
        let settings = MethodologySettings::kiro_default();
        assert_eq!(settings.rule_for("DONE").map(|r| r.name.as_str()), Some("DoneWhen"));
    }

    #[test]
    fn rule_for_unknown_tag_is_none() {
        assert!(MethodologySettings::kiro_default().rule_for("Nope").is_none());
    }

    #[test]
    fn override_task_list_file_replaces_default() {
        let settings = MethodologySettings::parse_overrides(r#"{"task_list_file":"plan.md"}"#).unwrap();
        assert_eq!(settings.task_list_file, "plan.md");
    }

    #[test]
    fn override_tag_replaces_same_named_default_entirely() {
        let json = r#"{"tags":[{"name":"Requirements","link":{"targets":[{"file":"spec.md"}],"locate":"title_text","values":"whole"}}]}"#;
        let settings = MethodologySettings::parse_overrides(json).unwrap();
        let rule = settings.rule_for("Requirements").unwrap();
        assert_eq!(rule.link.as_ref().unwrap().targets, vec![LinkTarget::File("spec.md".into())]);
        assert_eq!(rule.link.as_ref().unwrap().locate, LocateBy::TitleText);
        let requirements_count = settings.tags.iter().filter(|t| t.name == "Requirements").count();
        assert_eq!(requirements_count, 1);
    }

    #[test]
    fn override_adds_new_tag_name() {
        let json = r#"{"tags":[{"name":"Owner","aliases":["WHO"]}]}"#;
        let settings = MethodologySettings::parse_overrides(json).unwrap();
        assert_eq!(settings.rule_for("WHO").map(|r| r.name.as_str()), Some("Owner"));
        assert_eq!(settings.tags.len(), MethodologySettings::kiro_default().tags.len() + 1);
    }

    #[test]
    fn override_null_link_removes_the_link() {
        let json = r#"{"tags":[{"name":"Requirements","link":null}]}"#;
        let settings = MethodologySettings::parse_overrides(json).unwrap();
        assert_eq!(settings.rule_for("Requirements").unwrap().link, None);
    }

    #[test]
    fn override_leaves_unmentioned_defaults_intact() {
        let json = r#"{"tags":[{"name":"Owner"}]}"#;
        let settings = MethodologySettings::parse_overrides(json).unwrap();
        let defaults = MethodologySettings::kiro_default();
        assert_eq!(settings.task_list_file, defaults.task_list_file);
        assert_eq!(settings.rule_for("Depends"), defaults.rule_for("Depends"));
    }

    #[test]
    fn empty_object_equals_defaults() {
        assert_eq!(
            MethodologySettings::parse_overrides("{}").unwrap(),
            MethodologySettings::kiro_default()
        );
    }

    #[test]
    fn invalid_json_is_an_error() {
        let result = MethodologySettings::parse_overrides("{not json");
        assert!(matches!(result, Err(SettingsError::InvalidJson(_))));
    }

    #[test]
    fn wrong_shape_is_an_error() {
        let result = MethodologySettings::parse_overrides(r#"{"tags":"x"}"#);
        assert!(matches!(result, Err(SettingsError::InvalidJson(_))));
    }
}
