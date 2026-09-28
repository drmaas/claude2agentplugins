use std::collections::BTreeMap;
use std::path::Path;

use crate::claude::manifest::ClaudeManifest;
use crate::error::{Error, Result};

#[derive(Debug, Default, Clone)]
pub struct ClaudeSettings {
    pub agent: Option<String>,
    pub subagent_status_line: Option<serde_json::Value>,
    pub unknown_keys: Vec<String>,
    pub from_file: bool,
    pub from_manifest: bool,
}

pub fn load(plugin_root: &Path, manifest: &ClaudeManifest) -> Result<ClaudeSettings> {
    let mut settings = ClaudeSettings::default();
    if let Some(raw) = &manifest.settings {
        settings.from_manifest = true;
        apply_entries(&mut settings, raw.iter().map(|(k, v)| (k.as_str(), v)));
    }
    let path = plugin_root.join("settings.json");
    if path.is_file() {
        let content = std::fs::read_to_string(&path)?;
        let value: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| Error::Conversion(format!("Failed to parse {}: {e}", path.display())))?;
        settings.from_file = true;
        if let Some(object) = value.as_object() {
            // settings.json takes precedence over plugin.json settings.
            settings.agent = None;
            settings.subagent_status_line = None;
            settings.unknown_keys.clear();
            apply_entries(&mut settings, object.iter().map(|(k, v)| (k.as_str(), v)));
        } else {
            settings
                .unknown_keys
                .push("(non-object settings.json)".to_string());
        }
    }
    Ok(settings)
}

fn apply_entries<'a, I>(settings: &mut ClaudeSettings, entries: I)
where
    I: Iterator<Item = (&'a str, &'a serde_json::Value)>,
{
    let ordered: BTreeMap<&str, &serde_json::Value> = entries.collect();
    for (key, value) in ordered {
        match key {
            "agent" => {
                settings.agent = value.as_str().map(str::to_string).or_else(|| {
                    if value.is_null() {
                        None
                    } else {
                        Some(value.to_string().trim_matches('"').to_string())
                    }
                });
            }
            "subagentStatusLine" => {
                settings.subagent_status_line = Some((*value).clone());
            }
            other => {
                if !settings
                    .unknown_keys
                    .iter()
                    .any(|existing| existing == other)
                {
                    settings.unknown_keys.push(other.to_string());
                }
            }
        }
    }
}

pub fn warn_unmapped(settings: &ClaudeSettings, host: &str, warnings: &mut Vec<String>) {
    if !settings.from_file && !settings.from_manifest {
        return;
    }
    let source = if settings.from_file {
        "settings.json"
    } else {
        "plugin.json settings"
    };
    if let Some(agent) = &settings.agent {
        warnings.push(format!(
            "{source} key 'agent' ('{agent}') has no {host} equivalent; preserved in the sidecar"
        ));
    }
    if settings.subagent_status_line.is_some() {
        warnings.push(format!(
            "{source} key 'subagentStatusLine' has no {host} equivalent; preserved in the sidecar"
        ));
    }
    for key in &settings.unknown_keys {
        warnings.push(format!(
            "{source} key '{key}' is ignored by Claude Code and has no {host} equivalent; preserved in the sidecar"
        ));
    }
}
