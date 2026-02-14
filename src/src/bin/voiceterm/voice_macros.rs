//! Voice macro loading/matching so transcripts can expand before PTY injection.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use voiceterm::log_debug;

use crate::config::VoiceSendMode;

const DEFAULT_MACROS_RELATIVE_PATH: &str = ".voiceterm/macros.yaml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MacroExpansion {
    pub(crate) text: String,
    pub(crate) mode: VoiceSendMode,
    pub(crate) matched_trigger: Option<String>,
}

impl MacroExpansion {
    fn unchanged(text: &str, mode: VoiceSendMode) -> Self {
        Self {
            text: text.to_string(),
            mode,
            matched_trigger: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct VoiceMacros {
    rules: Vec<MacroRule>,
    source_path: Option<PathBuf>,
}

impl VoiceMacros {
    pub(crate) fn load_for_project(project_dir: &Path) -> Self {
        let path = project_dir.join(DEFAULT_MACROS_RELATIVE_PATH);
        Self::load_from_path(&path)
    }

    pub(crate) fn load_from_path(path: &Path) -> Self {
        let mut macros = Self {
            rules: Vec::new(),
            source_path: Some(path.to_path_buf()),
        };
        if !path.exists() {
            return macros;
        }
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(err) => {
                log_debug(&format!(
                    "voice macro file unreadable ({}): {err}",
                    path.display()
                ));
                return macros;
            }
        };
        match parse_rules(&contents) {
            Ok(rules) => {
                macros.rules = rules;
                log_debug(&format!(
                    "loaded {} voice macros from {}",
                    macros.rules.len(),
                    path.display()
                ));
            }
            Err(err) => {
                log_debug(&format!(
                    "voice macro file invalid ({}): {err}",
                    path.display()
                ));
            }
        }
        macros
    }

    pub(crate) fn len(&self) -> usize {
        self.rules.len()
    }

    pub(crate) fn source_path(&self) -> Option<&Path> {
        self.source_path.as_deref()
    }

    pub(crate) fn apply(&self, transcript: &str, default_mode: VoiceSendMode) -> MacroExpansion {
        if self.rules.is_empty() {
            return MacroExpansion::unchanged(transcript, default_mode);
        }
        let words: Vec<&str> = transcript.split_whitespace().collect();
        if words.is_empty() {
            return MacroExpansion::unchanged(transcript, default_mode);
        }
        let lowered_words: Vec<String> = words.iter().map(|word| normalize_word(word)).collect();

        let mut exact_match: Option<&MacroRule> = None;
        for rule in &self.rules {
            if !word_slice_eq(&lowered_words, &rule.trigger_words) {
                continue;
            }
            if exact_match
                .as_ref()
                .map(|current| rule.trigger_words.len() > current.trigger_words.len())
                .unwrap_or(true)
            {
                exact_match = Some(rule);
            }
        }
        if let Some(rule) = exact_match {
            return rule.expand("", default_mode);
        }

        let mut prefix_match: Option<(&MacroRule, String)> = None;
        for rule in &self.rules {
            if !rule.action.supports_prefix_match() {
                continue;
            }
            let trigger_len = rule.trigger_words.len();
            if lowered_words.len() <= trigger_len {
                continue;
            }
            if !word_slice_eq(&lowered_words[..trigger_len], &rule.trigger_words) {
                continue;
            }
            let remainder = words[trigger_len..].join(" ");
            if remainder.is_empty() {
                continue;
            }
            if prefix_match
                .as_ref()
                .map(|(current, _)| trigger_len > current.trigger_words.len())
                .unwrap_or(true)
            {
                prefix_match = Some((rule, remainder));
            }
        }
        if let Some((rule, remainder)) = prefix_match {
            return rule.expand(&remainder, default_mode);
        }

        MacroExpansion::unchanged(transcript, default_mode)
    }
}

fn normalize_word(word: &str) -> String {
    word.trim().to_ascii_lowercase()
}

fn word_slice_eq(words: &[String], trigger_words: &[String]) -> bool {
    words.len() == trigger_words.len() && words.iter().zip(trigger_words).all(|(a, b)| a == b)
}

#[derive(Debug, Clone)]
struct MacroRule {
    trigger_label: String,
    trigger_words: Vec<String>,
    action: MacroAction,
}

impl MacroRule {
    fn expand(&self, remainder: &str, default_mode: VoiceSendMode) -> MacroExpansion {
        MacroExpansion {
            text: self.action.render(remainder),
            mode: self.action.mode_override().unwrap_or(default_mode),
            matched_trigger: Some(self.trigger_label.clone()),
        }
    }
}

#[derive(Debug, Clone)]
enum MacroAction {
    Expansion {
        value: String,
        mode: Option<VoiceSendMode>,
    },
    Template {
        template: String,
        mode: Option<VoiceSendMode>,
    },
}

impl MacroAction {
    fn render(&self, remainder: &str) -> String {
        match self {
            MacroAction::Expansion { value, .. } => value.clone(),
            MacroAction::Template { template, .. } => template.replace("{TRANSCRIPT}", remainder),
        }
    }

    fn mode_override(&self) -> Option<VoiceSendMode> {
        match self {
            MacroAction::Expansion { mode, .. } | MacroAction::Template { mode, .. } => *mode,
        }
    }

    fn supports_prefix_match(&self) -> bool {
        match self {
            MacroAction::Template { template, .. } => template.contains("{TRANSCRIPT}"),
            MacroAction::Expansion { .. } => false,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawMacroFile {
    #[serde(default)]
    macros: BTreeMap<String, RawMacroEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawMacroEntry {
    Expansion(String),
    Object(RawMacroObject),
}

#[derive(Debug, Deserialize)]
struct RawMacroObject {
    #[serde(default)]
    expansion: Option<String>,
    #[serde(default)]
    template: Option<String>,
    #[serde(default)]
    mode: Option<RawMacroMode>,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RawMacroMode {
    Auto,
    Insert,
}

impl From<RawMacroMode> for VoiceSendMode {
    fn from(value: RawMacroMode) -> Self {
        match value {
            RawMacroMode::Auto => VoiceSendMode::Auto,
            RawMacroMode::Insert => VoiceSendMode::Insert,
        }
    }
}

fn parse_rules(raw: &str) -> Result<Vec<MacroRule>, String> {
    let parsed: RawMacroFile =
        serde_yaml::from_str(raw).map_err(|err| format!("yaml parse error: {err}"))?;
    let mut rules = Vec::new();
    for (trigger, entry) in parsed.macros {
        let trigger_label = trigger.trim().to_string();
        if trigger_label.is_empty() {
            return Err("macro trigger cannot be empty".to_string());
        }
        let trigger_words: Vec<String> = trigger_label
            .split_whitespace()
            .map(normalize_word)
            .collect();
        if trigger_words.is_empty() {
            return Err(format!(
                "macro trigger cannot be blank: {:?}",
                trigger_label
            ));
        }
        let action = match entry {
            RawMacroEntry::Expansion(value) => MacroAction::Expansion { value, mode: None },
            RawMacroEntry::Object(obj) => {
                let mode = obj.mode.map(Into::into);
                match (obj.template, obj.expansion) {
                    (Some(template), None) => MacroAction::Template { template, mode },
                    (None, Some(expansion)) => MacroAction::Expansion {
                        value: expansion,
                        mode,
                    },
                    (Some(_), Some(_)) => {
                        return Err(format!(
                            "macro '{trigger_label}' must set only one of template/expansion"
                        ));
                    }
                    (None, None) => {
                        return Err(format!(
                            "macro '{trigger_label}' must set template or expansion"
                        ));
                    }
                }
            }
        };
        rules.push(MacroRule {
            trigger_label,
            trigger_words,
            action,
        });
    }
    Ok(rules)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn parse_for_test(yaml: &str) -> VoiceMacros {
        VoiceMacros {
            rules: parse_rules(yaml).expect("valid yaml"),
            source_path: None,
        }
    }

    #[test]
    fn apply_exact_expansion_macro() {
        let macros = parse_for_test(
            r#"
macros:
  run tests: cargo test --all-features
"#,
        );

        let expanded = macros.apply(" Run   Tests ", VoiceSendMode::Auto);
        assert_eq!(expanded.text, "cargo test --all-features");
        assert_eq!(expanded.mode, VoiceSendMode::Auto);
        assert_eq!(expanded.matched_trigger.as_deref(), Some("run tests"));
    }

    #[test]
    fn apply_template_macro_uses_remainder_and_mode_override() {
        let macros = parse_for_test(
            r#"
macros:
  commit with message:
    template: "git commit -m '{TRANSCRIPT}'"
    mode: insert
"#,
        );

        let expanded = macros.apply(
            "commit with message fix login edge case",
            VoiceSendMode::Auto,
        );
        assert_eq!(expanded.text, "git commit -m 'fix login edge case'");
        assert_eq!(expanded.mode, VoiceSendMode::Insert);
        assert_eq!(
            expanded.matched_trigger.as_deref(),
            Some("commit with message")
        );
    }

    #[test]
    fn apply_returns_original_when_no_macro_matches() {
        let macros = parse_for_test(
            r#"
macros:
  run tests: cargo test
"#,
        );

        let expanded = macros.apply("status report please", VoiceSendMode::Insert);
        assert_eq!(expanded.text, "status report please");
        assert_eq!(expanded.mode, VoiceSendMode::Insert);
        assert!(expanded.matched_trigger.is_none());
    }

    #[test]
    fn parse_rejects_object_without_template_or_expansion() {
        let yaml = r#"
macros:
  bad:
    mode: auto
"#;
        let err = parse_rules(yaml).expect_err("invalid macro object");
        assert!(err.contains("must set template or expansion"));
    }

    #[test]
    fn load_for_project_reads_dot_voiceterm_macros_file() {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "voiceterm-macros-{}-{}",
            now,
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let macros_dir = dir.join(".voiceterm");
        fs::create_dir_all(&macros_dir).expect("create macro dir");
        let path = macros_dir.join("macros.yaml");
        fs::write(
            &path,
            r#"
macros:
  run tests: cargo test --all-features
"#,
        )
        .expect("write macros file");

        let macros = VoiceMacros::load_for_project(&dir);
        let expanded = macros.apply("run tests", VoiceSendMode::Auto);
        assert_eq!(expanded.text, "cargo test --all-features");
        assert_eq!(
            macros
                .source_path()
                .map(|p| p.ends_with(".voiceterm/macros.yaml")),
            Some(true)
        );

        let _ = fs::remove_dir_all(&dir);
    }
}
