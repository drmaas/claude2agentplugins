use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::agent_plugins::rules;
use crate::validate::name;

pub type Validation = (Vec<String>, Vec<String>);

const PLUGIN_TOP_LEVEL_FIELDS: &[&str] = &[
    "$schema",
    "name",
    "version",
    "description",
    "author",
    "homepage",
    "repository",
    "license",
    "keywords",
    "extensions",
];

const AUTHOR_FIELDS: &[&str] = &["name", "email", "url"];

pub fn validate_plugin(dir: &Path) -> Validation {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    let plugin_schema = validate_manifest(dir, &mut errors, &mut warnings);
    validate_skills(dir, &mut errors, &mut warnings);
    validate_mcp(dir, plugin_schema.as_deref(), &mut errors, &mut warnings);

    (errors, warnings)
}

fn validate_manifest(
    dir: &Path,
    errors: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> Option<String> {
    let manifest_path = dir.join("plugin.json");
    let content = match std::fs::read_to_string(&manifest_path) {
        Ok(c) => c,
        Err(_) => {
            errors.push("missing plugin.json".to_string());
            return None;
        }
    };
    let value: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            errors.push(format!("plugin.json is not valid JSON: {e}"));
            return None;
        }
    };

    let Some(obj) = value.as_object() else {
        errors.push("plugin.json must be a JSON object".to_string());
        return None;
    };

    for key in obj.keys() {
        if !PLUGIN_TOP_LEVEL_FIELDS.contains(&key.as_str()) {
            warnings.push(format!(
                "plugin.json unknown top-level field '{key}' ignored (§5.2)"
            ));
        }
    }

    let plugin_schema = match obj.get("$schema").and_then(|v| v.as_str()) {
        Some(s) if s == crate::agent_plugins::manifest::PLUGIN_SCHEMA => Some(s.to_string()),
        Some(s) => {
            errors.push(format!(
                "$schema '{s}' is not the Agent Plugins v1.0.0 schema"
            ));
            Some(s.to_string())
        }
        None => {
            errors.push("plugin.json is missing $schema".to_string());
            None
        }
    };

    match obj.get("name").and_then(|v| v.as_str()) {
        Some(n) if name::is_valid_plugin_name(n) => {}
        Some(n) => errors.push(format!("invalid plugin name '{n}'")),
        None => errors.push("plugin.json is missing name".to_string()),
    }

    if let Some(author) = obj.get("author") {
        match author.as_object() {
            Some(author_obj) => {
                for key in author_obj.keys() {
                    if !AUTHOR_FIELDS.contains(&key.as_str()) {
                        errors.push(format!(
                            "plugin.json author has unknown field '{key}' (§5.4)"
                        ));
                    } else if !author_obj.get(key).is_some_and(|v| v.is_string()) {
                        errors.push(format!("plugin.json author.{key} must be a string (§5.4)"));
                    }
                }
            }
            None => errors.push("plugin.json author must be an object (§5.4)".to_string()),
        }
    }

    if let Some(extensions) = obj.get("extensions") {
        match extensions.as_object() {
            Some(ext_obj) => {
                for (ns, value) in ext_obj {
                    if !name::is_valid_extension_namespace(ns) {
                        warnings.push(format!(
                            "extensions key '{ns}' is not a reverse-domain extension namespace"
                        ));
                    }
                    if !value.is_object() {
                        warnings.push(format!("extensions['{ns}'] must be an object; ignored"));
                    }
                }
            }
            None => warnings.push("extensions must be an object; ignored (§8.1)".to_string()),
        }
    }

    plugin_schema
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
        if let Some(compat) = fm.extra.get("compatibility") {
            match compat.as_str() {
                Some("") => {
                    errors.push(format!(
                        "skills/{dir_name}: compatibility must not be empty"
                    ));
                }
                Some(c) if c.chars().count() > 500 => {
                    errors.push(format!(
                        "skills/{dir_name}: compatibility exceeds 500 characters"
                    ));
                }
                Some(_) => {}
                None => {
                    errors.push(format!("skills/{dir_name}: compatibility must be a string"));
                }
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

fn validate_mcp(
    dir: &Path,
    plugin_schema: Option<&str>,
    errors: &mut Vec<String>,
    _warnings: &mut Vec<String>,
) {
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

    let Some(obj) = value.as_object() else {
        errors.push("mcp.json must be a JSON object".to_string());
        return;
    };

    for key in obj.keys() {
        if key != "$schema" && key != "mcpServers" {
            errors.push(format!("mcp.json unknown top-level field '{key}' (§7.2.1)"));
        }
    }

    let mcp_schema = match obj.get("$schema").and_then(|v| v.as_str()) {
        Some(s) if s == crate::agent_plugins::mcp::MCP_SCHEMA => Some(s),
        Some(s) => {
            errors.push(format!(
                "mcp.json $schema '{s}' is not the Agent Plugins v1.0.0 MCP schema"
            ));
            Some(s)
        }
        None => {
            errors.push("mcp.json is missing $schema".to_string());
            None
        }
    };

    if let (Some(plugin_schema), Some(mcp_schema)) = (plugin_schema, mcp_schema)
        && let (Some(pv), Some(mv)) = (
            rules::schema_version(plugin_schema),
            rules::schema_version(mcp_schema),
        )
        && pv != mv
    {
        errors.push(format!(
            "mcp.json $schema version '{mv}' does not match plugin.json version '{pv}' (§10.1)"
        ));
    }

    let Some(servers) = obj.get("mcpServers").and_then(|v| v.as_object()) else {
        errors.push("mcp.json is missing mcpServers object".to_string());
        return;
    };

    for (name, server) in servers {
        let Some(server_obj) = server.as_object() else {
            errors.push(format!("MCP server '{name}': entry must be an object"));
            continue;
        };
        let Some(server_type) = server_obj.get("type").and_then(|v| v.as_str()) else {
            errors.push(format!("MCP server '{name}': missing type"));
            continue;
        };
        match server_type {
            "stdio" => validate_stdio_server(name, server_obj, errors),
            "streamable-http" | "sse" => validate_remote_server(name, server_obj, errors),
            other => {
                errors.push(format!("MCP server '{name}': unknown type '{other}'"));
            }
        }
    }
}

fn validate_stdio_server(
    name: &str,
    server: &serde_json::Map<String, serde_json::Value>,
    errors: &mut Vec<String>,
) {
    const ALLOWED: &[&str] = &["type", "command", "args", "env", "cwd"];
    for key in server.keys() {
        if !ALLOWED.contains(&key.as_str()) {
            errors.push(format!(
                "MCP server '{name}': unknown field '{key}' for type stdio"
            ));
        }
    }

    let Some(command) = server.get("command").and_then(|v| v.as_str()) else {
        errors.push(format!("MCP server '{name}': missing command"));
        return;
    };
    if !rules::is_valid_stdio_command(command) {
        if command.contains("${") {
            errors.push(format!(
                "MCP server '{name}': command must not contain placeholders"
            ));
        } else if command.split_whitespace().count() > 1 {
            errors.push(format!(
                "MCP server '{name}': command must be a single executable token"
            ));
        } else {
            errors.push(format!(
                "MCP server '{name}': command must be a bare executable name or a './'-relative path (§7.2.1)"
            ));
        }
    }

    if let Some(args) = server.get("args") {
        match args.as_array() {
            Some(items) => {
                for (i, item) in items.iter().enumerate() {
                    if !item.is_string() {
                        errors.push(format!("MCP server '{name}': args[{i}] must be a string"));
                    }
                }
            }
            None => errors.push(format!("MCP server '{name}': args must be an array")),
        }
    }

    if let Some(env) = server.get("env") {
        match env.as_object() {
            Some(env_map) => {
                for (key, value) in env_map {
                    if key.eq_ignore_ascii_case("PLUGIN_ROOT")
                        || key.eq_ignore_ascii_case("PLUGIN_DATA")
                    {
                        errors.push(format!("MCP server '{name}': env key '{key}' is reserved"));
                    }
                    if !value.is_string() {
                        errors.push(format!(
                            "MCP server '{name}': env['{key}'] must be a string"
                        ));
                    }
                }
            }
            None => errors.push(format!("MCP server '{name}': env must be an object")),
        }
    }

    if let Some(cwd) = server.get("cwd") {
        match cwd.as_str() {
            Some(cwd) if rules::is_valid_stdio_cwd(cwd) => {}
            Some(_) => errors.push(format!(
                "MCP server '{name}': cwd must be './…', '${{PLUGIN_ROOT}}…', or '${{PLUGIN_DATA}}…' (§7.2.1)"
            )),
            None => errors.push(format!("MCP server '{name}': cwd must be a string")),
        }
    }
}

fn validate_remote_server(
    name: &str,
    server: &serde_json::Map<String, serde_json::Value>,
    errors: &mut Vec<String>,
) {
    const ALLOWED: &[&str] = &["type", "url", "headers"];
    for key in server.keys() {
        if !ALLOWED.contains(&key.as_str()) {
            errors.push(format!(
                "MCP server '{name}': unknown field '{key}' for remote transport"
            ));
        }
    }

    let Some(url) = server.get("url").and_then(|v| v.as_str()) else {
        errors.push(format!("MCP server '{name}': missing url"));
        return;
    };
    if let Err(msg) = rules::validate_remote_url(url) {
        errors.push(format!("MCP server '{name}': {msg}"));
    }

    if let Some(headers) = server.get("headers") {
        match headers.as_object() {
            Some(map) => {
                let mut seen = HashSet::new();
                for (key, value) in map {
                    let lower = key.to_ascii_lowercase();
                    if !seen.insert(lower) {
                        errors.push(format!(
                            "MCP server '{name}': duplicate header name '{key}' (header names are case-insensitive)"
                        ));
                    }
                    if !value.is_string() {
                        errors.push(format!(
                            "MCP server '{name}': header '{key}' value must be a string"
                        ));
                    }
                }
            }
            None => {
                errors.push(format!("MCP server '{name}': headers must be an object"));
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
                    "abs": {"type": "stdio", "command": "/usr/bin/node"},
                    "good": {"type": "stdio", "command": "./bin/x"},
                    "remote": {"type": "streamable-http", "url": "https://example.com/mcp"}
                }
            })
            .to_string(),
        )
        .unwrap();

        let (errors, _warnings) = validate_plugin(&dir);
        assert!(errors.iter().any(|e| e.contains("placeholders")));
        assert!(
            errors
                .iter()
                .any(|e| e.contains("'abs'") && e.contains("./"))
        );
        assert!(!errors.iter().any(|e| e.contains("'good'")));
        assert!(!errors.iter().any(|e| e.contains("'remote'")));
    }

    #[test]
    fn validates_cwd_and_http_loopback_rules() {
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
                    "cwd-bad": {
                        "type": "stdio",
                        "command": "npx",
                        "cwd": "/tmp"
                    },
                    "cwd-good": {
                        "type": "stdio",
                        "command": "npx",
                        "cwd": "${PLUGIN_DATA}/cache"
                    },
                    "http-remote": {
                        "type": "streamable-http",
                        "url": "http://example.com/mcp"
                    },
                    "http-local": {
                        "type": "sse",
                        "url": "http://localhost/mcp"
                    }
                }
            })
            .to_string(),
        )
        .unwrap();

        let (errors, _warnings) = validate_plugin(&dir);
        assert!(
            errors
                .iter()
                .any(|e| e.contains("'cwd-bad'") && e.contains("cwd"))
        );
        assert!(!errors.iter().any(|e| e.contains("'cwd-good'")));
        assert!(
            errors
                .iter()
                .any(|e| e.contains("'http-remote'") && e.contains("HTTPS"))
        );
        assert!(!errors.iter().any(|e| e.contains("'http-local'")));
    }

    #[test]
    fn reports_schema_version_mismatch_and_unknown_fields() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("plugin");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("plugin.json"),
            serde_json::json!({
                "$schema": crate::agent_plugins::manifest::PLUGIN_SCHEMA,
                "name": "my-plugin",
                "extraField": true
            })
            .to_string(),
        )
        .unwrap();
        std::fs::write(
            dir.join("mcp.json"),
            serde_json::json!({
                "$schema": "https://agent-plugins.org/schemas/1.1.0/mcp.schema.json",
                "mcpServers": {},
                "alsoExtra": 1
            })
            .to_string(),
        )
        .unwrap();

        let (errors, warnings) = validate_plugin(&dir);
        assert!(warnings.iter().any(|w| w.contains("extraField")));
        assert!(errors.iter().any(|e| e.contains("alsoExtra")));
        assert!(errors.iter().any(|e| e.contains("does not match")));
    }

    #[test]
    fn rejects_duplicate_header_casing() {
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
                    "remote": {
                        "type": "streamable-http",
                        "url": "https://example.com/mcp",
                        "headers": {
                            "X-Tenant": "a",
                            "x-tenant": "b"
                        }
                    }
                }
            })
            .to_string(),
        )
        .unwrap();

        let (errors, _warnings) = validate_plugin(&dir);
        assert!(errors.iter().any(|e| e.contains("duplicate header")));
    }
}
