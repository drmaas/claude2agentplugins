use std::path::{Path, PathBuf};

use crate::claude::manifest::{ClaudeManifest, StringOrArrayOrObject};
use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookDomain {
    Tool,
    Shell,
    Session,
    Permission,
}

#[derive(Debug, Clone)]
pub struct MappedHook {
    pub domain: HookDomain,
    pub hook_name: &'static str,
    pub claude_event: String,
    pub command: String,
    pub matcher: Option<String>,
    pub after_status: Option<&'static str>,
    pub notes: Vec<String>,
}

#[derive(Debug, Default)]
pub struct HooksConvert {
    pub mapped: Vec<MappedHook>,
    pub warnings: Vec<String>,
    pub needs_scripts: bool,
    pub source_files: Vec<PathBuf>,
}

pub fn convert(input: &Path, manifest: &ClaudeManifest) -> Result<HooksConvert> {
    let loaded = load_source(input, manifest)?;
    let Some(value) = loaded.value else {
        return Ok(HooksConvert {
            source_files: loaded.files,
            ..HooksConvert::default()
        });
    };
    let mut report = convert_value(&value);
    report.source_files = loaded.files;
    Ok(report)
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

pub fn convert_value(value: &serde_json::Value) -> HooksConvert {
    let mut report = HooksConvert::default();
    let hooks_obj = value.get("hooks").unwrap_or(value);
    let Some(hooks_obj) = hooks_obj.as_object() else {
        report.warnings.push(
            "hooks config is not an object; original preserved under extensions/".to_string(),
        );
        return report;
    };

    for (event, entries) in hooks_obj {
        let Some(entries) = entries.as_array() else {
            report
                .warnings
                .push(format!("Hook event '{event}' is not an array; skipped"));
            continue;
        };
        for entry in entries {
            convert_entry(event, entry, &mut report);
        }
    }

    if !report.mapped.is_empty() {
        report.warnings.push(format!(
            "Mapped {} Claude hook action(s) onto OpenCode v2 ctx.tool.hook / ctx.session.hook / ctx.shell.hook / ctx.permission.hook; command hooks run via child_process with a best-effort Claude stdin payload. Original hooks stay under extensions/",
            report.mapped.len()
        ));
    }

    report
}

fn convert_entry(event: &str, entry: &serde_json::Value, report: &mut HooksConvert) {
    if entry.get("hooks").is_some() {
        flatten_group(event, entry, report);
        return;
    }
    convert_action(event, None, entry, report);
}

fn flatten_group(event: &str, entry: &serde_json::Value, report: &mut HooksConvert) {
    let matcher = entry.get("matcher").and_then(|v| v.as_str());
    let Some(hooks) = entry.get("hooks").and_then(|v| v.as_array()) else {
        report.warnings.push(format!(
            "Hook event '{event}' group has no hooks array; skipped"
        ));
        return;
    };
    if entry.get("if").is_some() {
        report.warnings.push(format!(
            "Hook event '{event}': Claude 'if' conditions have no OpenCode v2 equivalent; dropped"
        ));
    }
    for hook in hooks {
        convert_action(event, matcher, hook, report);
    }
}

fn convert_action(
    event: &str,
    group_matcher: Option<&str>,
    hook: &serde_json::Value,
    report: &mut HooksConvert,
) {
    let kind = hook
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("command");
    if kind != "command" {
        report.warnings.push(format!(
            "Hook event '{event}': type '{kind}' cannot map onto OpenCode v2 plugin hooks (only type command is translated); preserved under extensions/"
        ));
        return;
    }
    let Some(command) = hook.get("command").and_then(|v| v.as_str()) else {
        report.warnings.push(format!(
            "Hook event '{event}': command hook is missing command; skipped"
        ));
        return;
    };

    let matcher = hook
        .get("matcher")
        .and_then(|v| v.as_str())
        .or(group_matcher)
        .filter(|m| !m.is_empty() && *m != ".*")
        .map(str::to_string);
    let rewritten = rewrite_command(command, event, report);
    if rewritten.contains("scripts/") || command.contains("scripts/") {
        report.needs_scripts = true;
    }

    let targets = map_targets(event, matcher.as_deref(), report);
    if targets.is_empty() {
        return;
    }

    for target in targets {
        let mut notes = Vec::new();
        notes.extend(target.notes);
        if hook.get("timeout").is_some() {
            notes.push(
                "Claude hook timeout is not applied on the OpenCode v2 registration".to_string(),
            );
        }
        if hook.get("args").is_some() {
            report.warnings.push(format!(
                "Hook event '{event}': args are not passed to OpenCode v2 hook runners; dropped"
            ));
        }
        report.mapped.push(MappedHook {
            domain: target.domain,
            hook_name: target.hook_name,
            claude_event: event.to_string(),
            command: rewritten.clone(),
            matcher: target.matcher.clone(),
            after_status: target.after_status,
            notes,
        });
    }
}

struct TargetSpec {
    domain: HookDomain,
    hook_name: &'static str,
    matcher: Option<String>,
    after_status: Option<&'static str>,
    notes: Vec<String>,
}

fn map_targets(event: &str, matcher: Option<&str>, report: &mut HooksConvert) -> Vec<TargetSpec> {
    match event {
        "PreToolUse" => map_pre_tool_use(matcher),
        "PostToolUse" => vec![TargetSpec {
            domain: HookDomain::Tool,
            hook_name: "execute.after",
            matcher: matcher.map(normalize_tool_matcher),
            after_status: Some("completed"),
            notes: vec![
                "PostToolUse mapped to ctx.tool.hook(\"execute.after\"); updatedToolOutput is applied when present"
                    .to_string(),
            ],
        }],
        "PostToolUseFailure" => vec![TargetSpec {
            domain: HookDomain::Tool,
            hook_name: "execute.after",
            matcher: matcher.map(normalize_tool_matcher),
            after_status: Some("error"),
            notes: vec![
                "PostToolUseFailure mapped to ctx.tool.hook(\"execute.after\") filtered to status error"
                    .to_string(),
            ],
        }],
        "UserPromptSubmit" => vec![TargetSpec {
            domain: HookDomain::Session,
            hook_name: "prompt",
            matcher: None,
            after_status: None,
            notes: vec![
                "UserPromptSubmit mapped to ctx.session.hook(\"prompt\"); Claude block decisions clear the prompt text as a best-effort rejection"
                    .to_string(),
            ],
        }],
        "PermissionRequest" => vec![TargetSpec {
            domain: HookDomain::Permission,
            hook_name: "evaluate",
            matcher: matcher.map(normalize_tool_matcher),
            after_status: None,
            notes: vec![
                "PermissionRequest mapped to ctx.permission.hook(\"evaluate\"); permissionDecision / decision.behavior become effect"
                    .to_string(),
            ],
        }],
        "PreCompact" => vec![TargetSpec {
            domain: HookDomain::Session,
            hook_name: "compaction",
            matcher: None,
            after_status: None,
            notes: vec![
                "PreCompact mapped to ctx.session.hook(\"compaction\"); Claude command hooks run for side effects only"
                    .to_string(),
            ],
        }],
        "Notification"
        | "SessionStart"
        | "SessionEnd"
        | "Stop"
        | "StopFailure"
        | "SubagentStart"
        | "SubagentStop"
        | "Setup"
        | "PostCompact"
        | "TeammateIdle"
        | "TaskCreated"
        | "TaskCompleted"
        | "WorktreeCreate"
        | "WorktreeRemove"
        | "PermissionDenied"
        | "PreModelSwitch"
        | "PostModelSwitch"
        | "ConfigChange"
        | "CwdChanged"
        | "DirectoryAdded"
        | "FileChanged"
        | "InstructionsLoaded"
        | "MessageDisplay"
        | "Elicitation"
        | "ElicitationResult"
        | "UserPromptExpansion"
        | "PostToolBatch" => {
            report.warnings.push(format!(
                "Hook event '{event}' has no faithful OpenCode v2 equivalent (ctx.tool.hook / ctx.session.hook / ctx.shell.hook / ctx.permission.hook); preserved under extensions/ only"
            ));
            Vec::new()
        }
        other => {
            report.warnings.push(format!(
                "Hook event '{other}' is unrecognized and was not mapped; preserved under extensions/"
            ));
            Vec::new()
        }
    }
}

fn map_pre_tool_use(matcher: Option<&str>) -> Vec<TargetSpec> {
    let mut targets = Vec::new();
    let parts = matcher
        .map(|m| {
            m.split('|')
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let bash_only = !parts.is_empty() && parts.iter().all(|p| eq_bash(p));
    let has_bash = parts.is_empty() || parts.iter().any(|p| eq_bash(p));
    let non_bash: Vec<&str> = parts.iter().copied().filter(|p| !eq_bash(p)).collect();

    if has_bash {
        targets.push(TargetSpec {
            domain: HookDomain::Shell,
            hook_name: "create.before",
            matcher: None,
            after_status: None,
            notes: vec![
                "PreToolUse Bash mapped to ctx.shell.hook(\"create.before\"); exit 2 / deny rewrites the command to `false` as a best-effort block"
                    .to_string(),
            ],
        });
    }

    if !bash_only {
        let tool_matcher = if non_bash.is_empty() {
            None
        } else {
            Some(normalize_tool_matcher(&non_bash.join("|")))
        };
        targets.push(TargetSpec {
            domain: HookDomain::Tool,
            hook_name: "execute.before",
            matcher: tool_matcher,
            after_status: None,
            notes: vec![
                "PreToolUse mapped to ctx.tool.hook(\"execute.before\"); updatedInput is applied when present; deny/exit 2 throws to abort as a best-effort block"
                    .to_string(),
            ],
        });
    }

    targets.push(TargetSpec {
        domain: HookDomain::Permission,
        hook_name: "evaluate",
        matcher: matcher.map(normalize_tool_matcher),
        after_status: None,
        notes: vec![
            "PreToolUse also registered on ctx.permission.hook(\"evaluate\") so allow/deny/ask decisions can set effect"
                .to_string(),
        ],
    });

    targets
}

fn eq_bash(token: &str) -> bool {
    token.eq_ignore_ascii_case("bash") || token.eq_ignore_ascii_case("shell")
}

fn normalize_tool_matcher(matcher: &str) -> String {
    matcher
        .split('|')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| match part.to_ascii_lowercase().as_str() {
            "bash" | "shell" => "bash".to_string(),
            "edit" | "write" | "multiedit" | "notebookedit" => "edit".to_string(),
            "read" | "readfile" => "read".to_string(),
            "glob" => "glob".to_string(),
            "grep" => "grep".to_string(),
            "ls" | "list" | "listdir" => "list".to_string(),
            "todowrite" | "todoread" => "todowrite".to_string(),
            "webfetch" => "webfetch".to_string(),
            "websearch" => "websearch".to_string(),
            "task" | "agent" => "task".to_string(),
            "skill" => "skill".to_string(),
            other => other.to_string(),
        })
        .collect::<Vec<_>>()
        .join("|")
}

fn rewrite_command(command: &str, event: &str, report: &mut HooksConvert) -> String {
    let mut value = command.to_string();
    if value.contains("${CLAUDE_PLUGIN_ROOT}") {
        report.warnings.push(format!(
            "Hook event '{event}' command: ${{CLAUDE_PLUGIN_ROOT}} rewritten to the OpenCode package root at runtime"
        ));
        value = value
            .replace("\"${CLAUDE_PLUGIN_ROOT}\"/", "")
            .replace("${CLAUDE_PLUGIN_ROOT}/", "./")
            .replace("\"${CLAUDE_PLUGIN_ROOT}\"", ".")
            .replace("${CLAUDE_PLUGIN_ROOT}", ".");
    }
    if value.contains("${CLAUDE_PLUGIN_DATA}") {
        report.warnings.push(format!(
            "Hook event '{event}' command: ${{CLAUDE_PLUGIN_DATA}} has no OpenCode v2 equivalent; left unchanged"
        ));
    }
    if value.contains("${CLAUDE_PROJECT_DIR}") {
        report.warnings.push(format!(
            "Hook event '{event}' command: ${{CLAUDE_PROJECT_DIR}} has no OpenCode v2 equivalent; left unchanged"
        ));
    }
    if value.starts_with("./") {
        // Keep relative; runner prefixes with package root.
    }
    value
}

pub fn copy_scripts(input: &Path, output: &Path) -> Result<bool> {
    let source = input.join("scripts");
    if !source.is_dir() {
        return Ok(false);
    }
    crate::convert::extensions::copy_entry(&source, &output.join("scripts"))?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_bash_pre_tool_use_to_shell_tool_and_permission() {
        let value = serde_json::json!({
            "hooks": {
                "PreToolUse": [{
                    "matcher": "Bash",
                    "hooks": [{ "type": "command", "command": "echo blocked" }]
                }],
                "Notification": [{
                    "hooks": [{ "type": "command", "command": "./scripts/notify.sh" }]
                }]
            }
        });
        let report = convert_value(&value);
        assert!(
            report
                .mapped
                .iter()
                .any(|h| h.domain == HookDomain::Shell && h.hook_name == "create.before")
        );
        assert!(
            !report
                .mapped
                .iter()
                .any(|h| h.domain == HookDomain::Tool && h.hook_name == "execute.before"),
            "Bash-only PreToolUse should not also register tool.execute.before"
        );
        assert!(
            report
                .mapped
                .iter()
                .any(|h| h.domain == HookDomain::Permission && h.hook_name == "evaluate")
        );
        assert!(report.warnings.iter().any(|w| w.contains("Notification")));
        assert!(
            !report
                .mapped
                .iter()
                .any(|h| h.claude_event == "Notification")
        );
    }

    #[test]
    fn maps_user_prompt_and_permission_request() {
        let value = serde_json::json!({
            "hooks": {
                "UserPromptSubmit": [{
                    "hooks": [{ "type": "command", "command": "./scripts/check.sh" }]
                }],
                "PermissionRequest": [{
                    "matcher": "Bash",
                    "hooks": [{ "type": "command", "command": "./scripts/ask.sh" }]
                }]
            }
        });
        let report = convert_value(&value);
        assert!(
            report
                .mapped
                .iter()
                .any(|h| h.domain == HookDomain::Session && h.hook_name == "prompt")
        );
        assert!(
            report
                .mapped
                .iter()
                .any(|h| h.domain == HookDomain::Permission && h.hook_name == "evaluate")
        );
        assert!(report.needs_scripts);
    }

    #[test]
    fn drops_prompt_hook_types() {
        let value = serde_json::json!({
            "hooks": {
                "Stop": [{
                    "hooks": [{ "type": "prompt", "prompt": "done?" }]
                }]
            }
        });
        let report = convert_value(&value);
        assert!(report.mapped.is_empty());
        assert!(report.warnings.iter().any(|w| w.contains("prompt")));
    }
}
