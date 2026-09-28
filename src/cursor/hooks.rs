use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::claude::manifest::{ClaudeManifest, StringOrArrayOrObject};
use crate::cursor::placeholders;
use crate::error::{Error, Result};

const EVENT_MAP: &[(&str, &str)] = &[
    ("PreToolUse", "preToolUse"),
    ("PostToolUse", "postToolUse"),
    ("PostToolUseFailure", "postToolUseFailure"),
    ("UserPromptSubmit", "beforeSubmitPrompt"),
    ("Stop", "stop"),
    ("SubagentStart", "subagentStart"),
    ("SubagentStop", "subagentStop"),
    ("SessionStart", "sessionStart"),
    ("SessionEnd", "sessionEnd"),
    ("PreCompact", "preCompact"),
];

const CURSOR_EVENTS: &[&str] = &[
    "sessionStart",
    "sessionEnd",
    "preToolUse",
    "postToolUse",
    "postToolUseFailure",
    "subagentStart",
    "subagentStop",
    "beforeShellExecution",
    "afterShellExecution",
    "beforeMCPExecution",
    "afterMCPExecution",
    "beforeReadFile",
    "afterFileEdit",
    "beforeSubmitPrompt",
    "preCompact",
    "stop",
    "afterAgentResponse",
    "afterAgentThought",
    "beforeTabFileRead",
    "afterTabFileEdit",
    "workspaceOpen",
];

const TOOL_EVENTS: &[&str] = &["preToolUse", "postToolUse", "postToolUseFailure"];

pub struct HooksConvert {
    pub config: Option<serde_json::Value>,
    pub warnings: Vec<String>,
    pub source_files: Vec<PathBuf>,
}

pub fn convert(input: &Path, manifest: &ClaudeManifest) -> Result<HooksConvert> {
    let loaded = load_source(input, manifest)?;
    let Some(value) = loaded.value else {
        return Ok(HooksConvert {
            config: None,
            warnings: Vec::new(),
            source_files: loaded.files,
        });
    };
    let (config, warnings) = convert_value(&value);
    Ok(HooksConvert {
        config,
        warnings,
        source_files: loaded.files,
    })
}

struct LoadedHooks {
    value: Option<serde_json::Value>,
    files: Vec<PathBuf>,
}

fn load_source(input: &Path, manifest: &ClaudeManifest) -> Result<LoadedHooks> {
    match &manifest.hooks {
        Some(StringOrArrayOrObject::Object(object)) => Ok(LoadedHooks {
            value: Some(serde_json::Value::Object(
                object.clone().into_iter().collect(),
            )),
            files: Vec::new(),
        }),
        Some(StringOrArrayOrObject::Single(path)) => read_hooks_file(input, path),
        Some(StringOrArrayOrObject::Multiple(paths)) => {
            let mut merged = serde_json::Map::new();
            let mut files = Vec::new();
            for path in paths {
                let loaded = read_hooks_file(input, path)?;
                files.extend(loaded.files);
                if let Some(serde_json::Value::Object(map)) = loaded.value {
                    for (key, value) in map {
                        merged.insert(key, value);
                    }
                }
            }
            Ok(LoadedHooks {
                value: Some(serde_json::Value::Object(merged)),
                files,
            })
        }
        None => {
            let path = input.join("hooks").join("hooks.json");
            if path.is_file() {
                let content = std::fs::read_to_string(&path)?;
                let value = serde_json::from_str(&content).map_err(|e| {
                    Error::Conversion(format!("Failed to parse {}: {e}", path.display()))
                })?;
                Ok(LoadedHooks {
                    value: Some(value),
                    files: vec![path],
                })
            } else {
                Ok(LoadedHooks {
                    value: None,
                    files: Vec::new(),
                })
            }
        }
    }
}

fn read_hooks_file(input: &Path, raw: &str) -> Result<LoadedHooks> {
    let cleaned = raw.trim_start_matches("./").trim_start_matches('/');
    let path = input.join(cleaned);
    if !path.is_file() {
        return Err(Error::Conversion(format!(
            "Hooks path '{}' does not exist",
            path.display()
        )));
    }
    let content = std::fs::read_to_string(&path)?;
    let value = serde_json::from_str(&content)
        .map_err(|e| Error::Conversion(format!("Failed to parse {}: {e}", path.display())))?;
    Ok(LoadedHooks {
        value: Some(value),
        files: vec![path],
    })
}

pub fn convert_value(value: &serde_json::Value) -> (Option<serde_json::Value>, Vec<String>) {
    let mut warnings = Vec::new();
    let hooks_obj = value.get("hooks").unwrap_or(value);
    let Some(hooks_obj) = hooks_obj.as_object() else {
        warnings
            .push("hooks config is not an object; original preserved in the sidecar".to_string());
        return (None, warnings);
    };

    let mut grouped: BTreeMap<String, Vec<serde_json::Value>> = BTreeMap::new();
    for (event, entries) in hooks_obj {
        let Some(cursor_event) = map_event(event) else {
            warnings.push(format!(
                "Hook event '{event}' has no Cursor equivalent; dropped from hooks/hooks.json (original preserved in the sidecar)"
            ));
            continue;
        };
        if !CURSOR_EVENTS.contains(&cursor_event) {
            warnings.push(format!(
                "Hook event '{event}' mapped to '{cursor_event}', which is not a documented Cursor hook; dropped"
            ));
            continue;
        }
        let Some(entries) = entries.as_array() else {
            warnings.push(format!("Hook event '{event}' is not an array; skipped"));
            continue;
        };
        for entry in entries {
            for converted in convert_entry(event, cursor_event, entry, &mut warnings) {
                grouped
                    .entry(cursor_event.to_string())
                    .or_default()
                    .push(converted);
            }
        }
    }

    if grouped.is_empty() {
        return (None, warnings);
    }
    let mut hooks = serde_json::Map::new();
    for (event, entries) in grouped {
        hooks.insert(event, serde_json::Value::Array(entries));
    }
    (
        Some(serde_json::json!({
            "version": 1,
            "hooks": hooks
        })),
        warnings,
    )
}

fn map_event(event: &str) -> Option<&str> {
    if let Some((_, cursor)) = EVENT_MAP.iter().find(|(claude, _)| *claude == event) {
        return Some(*cursor);
    }
    if CURSOR_EVENTS.contains(&event) {
        return Some(event);
    }
    None
}

fn convert_entry(
    source_event: &str,
    cursor_event: &str,
    entry: &serde_json::Value,
    warnings: &mut Vec<String>,
) -> Vec<serde_json::Value> {
    if entry.get("hooks").is_some() {
        return flatten_group(source_event, cursor_event, entry, warnings);
    }
    convert_action(source_event, cursor_event, None, entry, warnings)
        .into_iter()
        .collect()
}

fn flatten_group(
    source_event: &str,
    cursor_event: &str,
    entry: &serde_json::Value,
    warnings: &mut Vec<String>,
) -> Vec<serde_json::Value> {
    let matcher = entry.get("matcher").and_then(|v| v.as_str());
    let Some(hooks) = entry.get("hooks").and_then(|v| v.as_array()) else {
        warnings.push(format!(
            "Hook event '{source_event}' group has no hooks array; skipped"
        ));
        return Vec::new();
    };
    if entry.get("if").is_some() {
        warnings.push(format!(
            "Hook event '{source_event}': Claude 'if' conditions have no Cursor equivalent; dropped"
        ));
    }
    hooks
        .iter()
        .filter_map(|hook| convert_action(source_event, cursor_event, matcher, hook, warnings))
        .collect()
}

fn convert_action(
    source_event: &str,
    cursor_event: &str,
    group_matcher: Option<&str>,
    hook: &serde_json::Value,
    warnings: &mut Vec<String>,
) -> Option<serde_json::Value> {
    let kind = hook
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("command");
    if !matches!(kind, "command" | "prompt") {
        warnings.push(format!(
            "Hook event '{source_event}': type '{kind}' has no Cursor equivalent (command and prompt are supported); dropped"
        ));
        return None;
    }
    if hook.get("args").is_some() {
        warnings.push(format!(
            "Hook event '{source_event}': args are not a Cursor hook field; dropped (original preserved in the sidecar)"
        ));
    }

    let mut out = serde_json::Map::new();
    if kind == "prompt" {
        let Some(prompt) = hook.get("prompt").and_then(|v| v.as_str()) else {
            warnings.push(format!(
                "Hook event '{source_event}': prompt hook is missing prompt; skipped"
            ));
            return None;
        };
        out.insert("type".to_string(), serde_json::json!("prompt"));
        out.insert(
            "prompt".to_string(),
            serde_json::Value::String(prompt.to_string()),
        );
    } else {
        let Some(command) = hook.get("command").and_then(|v| v.as_str()) else {
            warnings.push(format!(
                "Hook event '{source_event}': command hook is missing command; skipped"
            ));
            return None;
        };
        let rewritten = placeholders::rewrite_hook_command(
            command,
            &format!("Hook event '{source_event}' command"),
        );
        warnings.extend(rewritten.warnings);
        out.insert(
            "command".to_string(),
            serde_json::Value::String(rewritten.value),
        );
    }

    if let Some(timeout) = hook.get("timeout").filter(|v| v.is_number()) {
        out.insert("timeout".to_string(), timeout.clone());
    }

    let matcher = hook
        .get("matcher")
        .and_then(|v| v.as_str())
        .or(group_matcher);
    if let Some(matcher) = matcher
        && !matcher.is_empty()
        && matcher != ".*"
    {
        let rewritten = if TOOL_EVENTS.contains(&cursor_event) {
            rewrite_tool_matcher(matcher, cursor_event, warnings)
        } else {
            matcher.to_string()
        };
        if !rewritten.is_empty() {
            out.insert("matcher".to_string(), serde_json::Value::String(rewritten));
        }
    }

    Some(serde_json::Value::Object(out))
}

fn rewrite_tool_matcher(matcher: &str, event: &str, warnings: &mut Vec<String>) -> String {
    let mut parts = Vec::new();
    for part in matcher.split('|') {
        let token = part.trim();
        let mapped = match token {
            "Bash" => "Shell",
            "Edit" => "Write",
            "Glob" => {
                warnings.push(format!(
                    "Hook event '{event}': matcher tool 'Glob' has no Cursor equivalent; left unchanged"
                ));
                "Glob"
            }
            other => other,
        };
        if parts.last().is_none_or(|prev: &String| prev != mapped) {
            parts.push(mapped.to_string());
        }
    }
    parts.join("|")
}

pub fn write(config: &serde_json::Value, output_dir: &Path) -> Result<()> {
    let path = output_dir.join("hooks").join("hooks.json");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(config)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_pre_tool_use_and_drops_notification() {
        let value = serde_json::json!({
            "hooks": {
                "PreToolUse": [{
                    "matcher": "Bash|Edit",
                    "hooks": [{
                        "type": "command",
                        "command": "\"${CLAUDE_PLUGIN_ROOT}\"/scripts/check.sh"
                    }]
                }],
                "Notification": [{
                    "hooks": [{"type": "command", "command": "./scripts/notify.sh"}]
                }]
            }
        });
        let (config, warnings) = convert_value(&value);
        let config = config.unwrap();
        let entry = &config["hooks"]["preToolUse"][0];
        assert_eq!(entry["command"], "./scripts/check.sh");
        assert_eq!(entry["matcher"], "Shell|Write");
        assert!(config["hooks"].get("Notification").is_none());
        assert!(warnings.iter().any(|w| w.contains("Notification")));
    }

    #[test]
    fn maps_prompt_hook() {
        let value = serde_json::json!({
            "hooks": {
                "Stop": [{
                    "hooks": [{
                        "type": "prompt",
                        "prompt": "Did the task finish?"
                    }]
                }]
            }
        });
        let (config, _) = convert_value(&value);
        let entry = &config.unwrap()["hooks"]["stop"][0];
        assert_eq!(entry["type"], "prompt");
        assert_eq!(entry["prompt"], "Did the task finish?");
    }
}
