use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

use crate::validate::name;

pub type Validation = (Vec<String>, Vec<String>);

pub fn validate_plugin(dir: &Path) -> Validation {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    validate_manifest(dir, &mut errors, &mut warnings);
    validate_skills(dir, &mut errors, &mut warnings);
    validate_mcp(dir, &mut errors, &mut warnings);

    (errors, warnings)
}

fn validate_manifest(dir: &Path, errors: &mut Vec<String>, warnings: &mut Vec<String>) {
    let manifest_path = dir.join("plugin.json");
    let content = match std::fs::read_to_string(&manifest_path) {
        Ok(c) => c,
        Err(_) => {
            errors.push("missing plugin.json".to_string());
            return;
        }
    };
    let value: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            errors.push(format!("plugin.json is not valid JSON: {e}"));
            return;
        }
    };

    match value.get("$schema").and_then(|v| v.as_str()) {
        Some(s) if s == crate::agent_plugins::manifest::PLUGIN_SCHEMA => {}
        Some(s) => errors.push(format!(
            "$schema '{s}' is not the Agent Plugins v1.0.0 schema"
        )),
        None => errors.push("plugin.json is missing $schema".to_string()),
    }

    match value.get("name").and_then(|v| v.as_str()) {
        Some(n) if name::is_valid_plugin_name(n) => {}
        Some(n) => errors.push(format!("invalid plugin name '{n}'")),
        None => errors.push("plugin.json is missing name".to_string()),
    }

    if let Some(extensions) = value.get("extensions")
        && !extensions.is_object()
    {
        warnings.push("extensions must be an object; ignored".to_string());
    }
}

#[derive(Debug, Deserialize)]
struct SkillFrontmatter {
    name: String,
    description: Option<String>,
    #[serde(rename = "allowed-tools")]
    allowed_tools: Option<serde_yaml::Value>,
    #[serde(flatten)]
    extra: HashMap<String, serde_yaml::Value>,
}

fn parse_skill_frontmatter(content: &str) -> Option<(SkillFrontmatter, String)> {
    let content = content.trim_start();
    if !content.starts_with("---") {
        return None;
    }
    let rest = &content[3..];
    let end = rest.find("---")?;
    let yaml = &rest[..end];
    let body = rest[end + 3..].trim().to_string();
    let fm: SkillFrontmatter = serde_yaml::from_str(yaml).ok()?;
    Some((fm, body))
}

fn validate_skills(dir: &Path, errors: &mut Vec<String>, warnings: &mut Vec<String>) {
    let skills_dir = dir.join("skills");
    if !skills_dir.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(&skills_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let dir_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let skill_md = path.join("SKILL.md");
        if !skill_md.is_file() {
            warnings.push(format!("skills/{dir_name} has no SKILL.md"));
            continue;
        }
        let content = match std::fs::read_to_string(&skill_md) {
            Ok(c) => c,
            Err(e) => {
                errors.push(format!("skills/{dir_name}/SKILL.md unreadable: {e}"));
                continue;
            }
        };
        let Some((fm, _body)) = parse_skill_frontmatter(&content) else {
            errors.push(format!(
                "skills/{dir_name}/SKILL.md has no parseable frontmatter"
            ));
            continue;
        };

        if !name::is_valid_skill_name(&fm.name) {
            errors.push(format!(
                "skills/{dir_name}: invalid skill name '{}'",
                fm.name
            ));
        }
        if fm.name != dir_name {
            errors.push(format!(
                "skills/{dir_name}: frontmatter name '{}' does not match directory name",
                fm.name
            ));
        }
        match &fm.description {
            Some(d) if d.is_empty() => {
                errors.push(format!("skills/{dir_name}: description must not be empty"));
            }
            Some(d) if d.chars().count() > 1024 => {
                errors.push(format!(
                    "skills/{dir_name}: description exceeds 1024 characters"
                ));
            }
            Some(_) => {}
            None => {
                errors.push(format!("skills/{dir_name}: missing description"));
            }
        }
        if let Some(meta) = fm.extra.get("metadata") {
            if let Some(map) = meta.as_mapping() {
                for (k, v) in map {
                    if !v.is_string() {
                        warnings.push(format!(
                            "skills/{dir_name}: metadata value '{k:?}' is not a string ({v:?})"
                        ));
                    }
                }
            } else {
                warnings.push(format!("skills/{dir_name}: metadata must be a map"));
            }
        }
        if let Some(tools) = &fm.allowed_tools
            && !tools.is_string()
        {
            warnings.push(format!(
                "skills/{dir_name}: allowed-tools should be a space-separated string"
            ));
        }
    }
}

fn validate_mcp(dir: &Path, errors: &mut Vec<String>, _warnings: &mut Vec<String>) {
    let mcp_path = dir.join("mcp.json");
    if !mcp_path.is_file() {
        return;
    }
    let content = match std::fs::read_to_string(&mcp_path) {
        Ok(c) => c,
        Err(e) => {
            errors.push(format!("mcp.json unreadable: {e}"));
            return;
        }
    };
    let value: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            errors.push(format!("mcp.json is not valid JSON: {e}"));
            return;
        }
    };

    match value.get("$schema").and_then(|v| v.as_str()) {
        Some(s) if s == crate::agent_plugins::mcp::MCP_SCHEMA => {}
        Some(s) => errors.push(format!(
            "mcp.json $schema '{s}' is not the Agent Plugins v1.0.0 MCP schema"
        )),
        None => errors.push("mcp.json is missing $schema".to_string()),
    }

    let Some(servers) = value.get("mcpServers").and_then(|v| v.as_object()) else {
        errors.push("mcp.json is missing mcpServers object".to_string());
        return;
    };

    for (name, server) in servers {
        let Some(server_type) = server.get("type").and_then(|v| v.as_str()) else {
            errors.push(format!("MCP server '{name}': missing type"));
            continue;
        };
        match server_type {
            "stdio" => {
                let Some(command) = server.get("command").and_then(|v| v.as_str()) else {
                    errors.push(format!("MCP server '{name}': missing command"));
                    continue;
                };
                if command.is_empty() {
                    errors.push(format!("MCP server '{name}': empty command"));
                }
                if command.contains("${") {
                    errors.push(format!(
                        "MCP server '{name}': command must not contain placeholders"
                    ));
                }
                if command.split_whitespace().count() > 1 {
                    errors.push(format!(
                        "MCP server '{name}': command must be a single executable token"
                    ));
                }
                if let Some(env) = server.get("env").and_then(|v| v.as_object()) {
                    for key in env.keys() {
                        if key.eq_ignore_ascii_case("PLUGIN_ROOT")
                            || key.eq_ignore_ascii_case("PLUGIN_DATA")
                        {
                            errors
                                .push(format!("MCP server '{name}': env key '{key}' is reserved"));
                        }
                    }
                }
            }
            "streamable-http" | "sse" => {
                let Some(url) = server.get("url").and_then(|v| v.as_str()) else {
                    errors.push(format!("MCP server '{name}': missing url"));
                    continue;
                };
                if url.contains("${") {
                    errors.push(format!(
                        "MCP server '{name}': url contains a ${{...}} placeholder which Agent Plugins does not expand; use a literal absolute URL"
                    ));
                } else if !(url.starts_with("https://") || url.starts_with("http://")) {
                    errors.push(format!("MCP server '{name}': url must be absolute HTTP(S)"));
                }
                if url.contains('@') || url.contains('#') {
                    errors.push(format!(
                        "MCP server '{name}': url must not contain user info or a fragment"
                    ));
                }
            }
            other => {
                errors.push(format!("MCP server '{name}': unknown type '{other}'"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_plugin_passes() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("plugin");
        std::fs::create_dir_all(dir.join("skills").join("good")).unwrap();
        std::fs::write(
            dir.join("plugin.json"),
            serde_json::json!({
                "$schema": crate::agent_plugins::manifest::PLUGIN_SCHEMA,
                "name": "my-plugin"
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            dir.join("skills").join("good").join("SKILL.md"),
            "---\nname: good\ndescription: A good skill\n---\n\nBody.",
        )
        .unwrap();

        let (errors, warnings) = validate_plugin(&dir);
        assert!(errors.is_empty(), "{errors:?}");
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn reports_invalid_name_and_missing_description() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("plugin");
        std::fs::create_dir_all(dir.join("skills").join("Bad_Name")).unwrap();
        std::fs::write(
            dir.join("plugin.json"),
            serde_json::json!({
                "$schema": "https://example.com/wrong-schema",
                "name": "Bad Name!"
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            dir.join("skills").join("Bad_Name").join("SKILL.md"),
            "---\nname: Bad_Name\n---\n\nBody.",
        )
        .unwrap();

        let (errors, _warnings) = validate_plugin(&dir);
        assert!(errors.iter().any(|e| e.contains("$schema")));
        assert!(errors.iter().any(|e| e.contains("invalid plugin name")));
        assert!(errors.iter().any(|e| e.contains("missing description")));
    }

    #[test]
    fn validates_mcp_command_shape() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("plugin");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("plugin.json"),
            serde_json::json!({
                "$schema": crate::agent_plugins::manifest::PLUGIN_SCHEMA,
                "name": "my-plugin"
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            dir.join("mcp.json"),
            serde_json::json!({
                "$schema": crate::agent_plugins::mcp::MCP_SCHEMA,
                "mcpServers": {
                    "bad": {"type": "stdio", "command": "${PLUGIN_ROOT}/bin/x"},
                    "good": {"type": "stdio", "command": "./bin/x"},
                    "remote": {"type": "streamable-http", "url": "https://example.com/mcp"}
                }
            })
            .to_string(),
        )
        .unwrap();

        let (errors, _warnings) = validate_plugin(&dir);
        assert!(errors.iter().any(|e| e.contains("placeholders")));
        assert!(!errors.iter().any(|e| e.contains("'good'")));
        assert!(!errors.iter().any(|e| e.contains("'remote'")));
    }
}
