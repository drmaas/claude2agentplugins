use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use crate::claude::manifest::{ClaudeManifest, StringOrArray};
use crate::cursor::markdown::{self, insert_string, yaml_bool};
use crate::cursor::placeholders;
use crate::error::Result;
use crate::validate::name;

const CURSOR_FIELDS: &[&str] = &["name", "description", "model", "readonly", "is_background"];

const CLAUDE_ONLY: &[&str] = &[
    "effort",
    "maxTurns",
    "tools",
    "disallowedTools",
    "skills",
    "memory",
    "background",
    "isolation",
    "hooks",
    "mcpServers",
    "permissionMode",
    "color",
    "initialPrompt",
];

const CLAUDE_MODEL_ALIASES: &[&str] = &["sonnet", "opus", "haiku", "fable"];

const READ_ONLY_TOOLS: &[&str] = &["read", "readfile", "grep", "glob", "ls", "list", "listdir"];

pub struct WrittenAgent {
    pub name: String,
    pub warnings: Vec<String>,
    pub extras: BTreeMap<String, serde_json::Value>,
}

pub fn convert(
    input: &Path,
    manifest: &ClaudeManifest,
    output: &Path,
) -> Result<Vec<WrittenAgent>> {
    let files = agent_files(input, manifest);
    let mut written = Vec::new();
    let mut used = HashSet::new();
    for path in files {
        if let Some(agent) = write_agent(&path, output, &mut used)? {
            written.push(agent);
        }
    }
    Ok(written)
}

fn agent_files(input: &Path, manifest: &ClaudeManifest) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(paths) = &manifest.agents {
        match paths {
            StringOrArray::Single(p) => push_agent_path(input, p, &mut dirs),
            StringOrArray::Multiple(list) => {
                for p in list {
                    push_agent_path(input, p, &mut dirs);
                }
            }
        }
    }
    let default = input.join("agents");
    if default.is_dir() && !dirs.iter().any(|d| d == &default) {
        dirs.push(default);
    }
    let mut files = Vec::new();
    for dir in dirs {
        collect_markdown(&dir, &mut files);
    }
    files.sort();
    files.dedup();
    files
}

fn push_agent_path(input: &Path, raw: &str, dirs: &mut Vec<PathBuf>) {
    let cleaned = raw.trim_start_matches("./").trim_start_matches('/');
    if cleaned.is_empty() {
        return;
    }
    let resolved = input.join(cleaned);
    if resolved.is_file() || resolved.is_dir() {
        dirs.push(resolved);
    }
}

fn collect_markdown(path: &Path, files: &mut Vec<PathBuf>) {
    if path.is_file() && is_markdown(path) {
        files.push(path.to_path_buf());
        return;
    }
    if !path.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    let mut paths: Vec<_> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_file() && is_markdown(&path) {
            files.push(path);
        }
    }
}

fn is_markdown(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("md" | "mdc" | "markdown")
    )
}

fn write_agent(
    path: &Path,
    output: &Path,
    used: &mut HashSet<String>,
) -> Result<Option<WrittenAgent>> {
    let raw = std::fs::read_to_string(path)?;
    let doc = markdown::parse(&raw);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("agent");
    let mut warnings = Vec::new();
    let mut extras = BTreeMap::new();
    let mut handled = HashSet::from(["name".to_string(), "description".to_string()]);

    let source_name = doc
        .frontmatter
        .as_ref()
        .and_then(|fm| markdown::mapping_string(fm, "name"))
        .unwrap_or_else(|| stem.to_string());
    let normalized = name::normalize_skill_name(&source_name);
    if normalized != source_name {
        warnings.push(format!(
            "Agent '{source_name}': renamed to '{normalized}' (Cursor agents use lowercase kebab-case)"
        ));
    }
    if !used.insert(normalized.clone()) {
        warnings.push(format!(
            "Agent '{normalized}': name already used; skipped {stem}"
        ));
        return Ok(Some(WrittenAgent {
            name: normalized,
            warnings,
            extras,
        }));
    }

    let description = doc
        .frontmatter
        .as_ref()
        .and_then(|fm| markdown::mapping_string(fm, "description"))
        .filter(|d| !d.trim().is_empty())
        .or_else(|| markdown::first_prose_line(&doc.body))
        .unwrap_or_else(|| format!("Converted Claude agent '{normalized}'."));
    if doc
        .frontmatter
        .as_ref()
        .and_then(|fm| markdown::mapping_string(fm, "description"))
        .is_none()
    {
        warnings.push(format!(
            "Agent '{stem}': missing description; synthesized one so Cursor will load the agent"
        ));
    }

    let mut front = serde_yaml::Mapping::new();
    insert_string(&mut front, "name", normalized.clone());
    insert_string(&mut front, "description", description);

    let mut cursor_model = None;
    if let Some(fm) = &doc.frontmatter {
        if let Some(raw_model) = markdown::mapping_string(fm, "model") {
            handled.insert("model".to_string());
            match map_cursor_model(&raw_model) {
                Some(model) => cursor_model = Some(model),
                None => {
                    warnings.push(format!(
                        "Agent '{stem}': Claude model alias '{raw_model}' has no documented Cursor model id; preserved in the sidecar"
                    ));
                    extras.insert("model".to_string(), serde_json::Value::String(raw_model));
                }
            }
        }

        if let Some(effort) = markdown::mapping_string(fm, "effort") {
            handled.insert("effort".to_string());
            if let Some(model) = cursor_model.as_mut() {
                *model = apply_effort_to_model(model, &effort);
                warnings.push(format!(
                    "Agent '{stem}': Claude effort '{effort}' mapped onto Cursor model parameter [effort={effort}]"
                ));
            } else {
                warnings.push(format!(
                    "Agent '{stem}': Claude effort '{effort}' requires a Cursor-mapped model to apply [effort=...]; preserved in the sidecar"
                ));
                extras.insert("effort".to_string(), serde_json::Value::String(effort));
            }
        }

        if let Some(model) = cursor_model {
            insert_string(&mut front, "model", model);
        }

        if let Some(value) = markdown::mapping_get(fm, "background") {
            handled.insert("background".to_string());
            if let Some(flag) = yaml_bool(value) {
                front.insert(
                    serde_yaml::Value::String("is_background".to_string()),
                    serde_yaml::Value::Bool(flag),
                );
                warnings.push(format!(
                    "Agent '{stem}': Claude background mapped to Cursor is_background"
                ));
            } else {
                warnings.push(format!(
                    "Agent '{stem}': Claude background is not a boolean; preserved in the sidecar"
                ));
                if let Ok(json) = serde_json::to_value(value) {
                    extras.insert("background".to_string(), json);
                }
            }
        }

        if let Some(tools_value) = markdown::mapping_get(fm, "tools") {
            handled.insert("tools".to_string());
            let tools = tool_names(tools_value);
            if tools_imply_readonly(&tools) {
                front.insert(
                    serde_yaml::Value::String("readonly".to_string()),
                    serde_yaml::Value::Bool(true),
                );
                warnings.push(format!(
                    "Agent '{stem}': Claude tools allowlist is read-only; mapped to Cursor readonly: true (Cursor has no tools frontmatter)"
                ));
            } else {
                warnings.push(format!(
                    "Agent '{stem}': Claude tools has no Cursor agent field (only readonly is documented); preserved in the sidecar"
                ));
            }
            if let Ok(json) = serde_json::to_value(tools_value) {
                extras.insert("tools".to_string(), json);
            }
        }

        if let Some(map) = fm.as_mapping() {
            for (key, value) in map {
                let Some(key) = key.as_str() else { continue };
                if handled.contains(key) || CURSOR_FIELDS.contains(&key) {
                    continue;
                }
                let label = if CLAUDE_ONLY.contains(&key) {
                    "Claude-only"
                } else {
                    "unmapped"
                };
                warnings.push(format!(
                    "Agent '{stem}': {label} frontmatter '{key}' is not part of Cursor's agent schema (name, description, model, readonly, is_background); preserved in the sidecar"
                ));
                if let Ok(json) = serde_json::to_value(value) {
                    extras.insert(key.to_string(), json);
                }
            }
        }
    }

    let body = placeholders::rewrite_placeholders(&doc.body, &format!("Agent '{stem}'"));
    warnings.extend(body.warnings);

    markdown::write(
        &output.join("agents").join(format!("{normalized}.md")),
        &serde_yaml::Value::Mapping(front),
        &body.value,
    )?;

    Ok(Some(WrittenAgent {
        name: normalized,
        warnings,
        extras,
    }))
}

fn map_cursor_model(raw: &str) -> Option<String> {
    let model = raw.trim();
    if model.is_empty() {
        return None;
    }
    if model.eq_ignore_ascii_case("inherit") {
        return Some("inherit".to_string());
    }
    if CLAUDE_MODEL_ALIASES
        .iter()
        .any(|alias| model.eq_ignore_ascii_case(alias))
    {
        return None;
    }
    Some(model.to_string())
}

fn apply_effort_to_model(model: &str, effort: &str) -> String {
    let effort = effort.trim();
    if let Some((base, params)) = model.split_once('[') {
        let params = params.trim_end_matches(']');
        if params
            .split(',')
            .any(|part| part.trim().starts_with("effort="))
        {
            return model.to_string();
        }
        if params.is_empty() {
            format!("{base}[effort={effort}]")
        } else {
            format!("{base}[{params},effort={effort}]")
        }
    } else {
        format!("{model}[effort={effort}]")
    }
}

fn tool_names(value: &serde_yaml::Value) -> Vec<String> {
    match value {
        serde_yaml::Value::String(text) => split_tools(text),
        serde_yaml::Value::Sequence(items) => items
            .iter()
            .flat_map(|item| match item {
                serde_yaml::Value::String(text) => split_tools(text),
                serde_yaml::Value::Number(number) => vec![number.to_string()],
                serde_yaml::Value::Bool(flag) => vec![flag.to_string()],
                _ => Vec::new(),
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn split_tools(text: &str) -> Vec<String> {
    if text.contains(',') {
        text.split(',')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect()
    } else {
        text.split_whitespace().map(str::to_string).collect()
    }
}

fn tools_imply_readonly(tools: &[String]) -> bool {
    !tools.is_empty()
        && tools.iter().all(|tool| {
            let base = tool
                .split(['(', ':'])
                .next()
                .unwrap_or(tool)
                .trim()
                .to_ascii_lowercase();
            READ_ONLY_TOOLS.contains(&base.as_str())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_inherit_background_and_readonly_tools() {
        let temp = tempfile::tempdir().unwrap();
        let agents = temp.path().join("agents");
        std::fs::create_dir_all(&agents).unwrap();
        std::fs::write(
            agents.join("security-reviewer.md"),
            "---\nname: security-reviewer\ndescription: Reviews code.\nmodel: inherit\neffort: high\nbackground: true\ntools: Read, Grep\n---\n\nBe careful.\n",
        )
        .unwrap();
        let output = temp.path().join("out");
        let written = convert(temp.path(), &ClaudeManifest::synthesize("p"), &output).unwrap();
        assert_eq!(written.len(), 1);
        let content = std::fs::read_to_string(output.join("agents/security-reviewer.md")).unwrap();
        assert!(content.contains("model: inherit[effort=high]"));
        assert!(content.contains("is_background: true"));
        assert!(content.contains("readonly: true"));
        assert!(content.contains("Be careful."));
        assert!(!content.contains("tools:"));
        assert!(written[0].extras.contains_key("tools"));
    }

    #[test]
    fn keeps_claude_model_alias_in_sidecar() {
        let temp = tempfile::tempdir().unwrap();
        let agents = temp.path().join("agents");
        std::fs::create_dir_all(&agents).unwrap();
        std::fs::write(
            agents.join("security-reviewer.md"),
            "---\nname: security-reviewer\ndescription: Reviews code.\nmodel: sonnet\n---\n\nBe careful.\n",
        )
        .unwrap();
        let output = temp.path().join("out");
        let written = convert(temp.path(), &ClaudeManifest::synthesize("p"), &output).unwrap();
        let content = std::fs::read_to_string(output.join("agents/security-reviewer.md")).unwrap();
        assert!(!content.contains("sonnet"));
        assert!(
            written[0]
                .warnings
                .iter()
                .any(|w| w.contains("model alias"))
        );
        assert_eq!(
            written[0].extras.get("model").and_then(|v| v.as_str()),
            Some("sonnet")
        );
    }
}
