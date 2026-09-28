use std::path::Path;

use crate::validate::name;

const CURSOR_HOOKS: &[&str] = &[
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

pub fn validate_plugin(dir: &Path) -> (Vec<String>, Vec<String>) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    validate_manifest(dir, &mut errors);
    validate_skills(dir, &mut errors, &mut warnings);
    validate_rules(dir, &mut errors, &mut warnings);
    validate_agents(dir, &mut warnings);
    validate_commands(dir, &mut warnings);
    validate_mcp(dir, &mut errors);
    validate_hooks(dir, &mut errors, &mut warnings);
    (errors, warnings)
}

fn validate_manifest(dir: &Path, errors: &mut Vec<String>) {
    let path = dir.join(".cursor-plugin").join("plugin.json");
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(_) => {
            errors.push("missing .cursor-plugin/plugin.json".to_string());
            return;
        }
    };
    let value: serde_json::Value = match serde_json::from_str(&content) {
        Ok(value) => value,
        Err(err) => {
            errors.push(format!(
                ".cursor-plugin/plugin.json is not valid JSON: {err}"
            ));
            return;
        }
    };
    match value.get("name").and_then(|v| v.as_str()) {
        Some(plugin_name) if name::is_valid_plugin_name(plugin_name) => {}
        Some(plugin_name) => errors.push(format!("invalid plugin name '{plugin_name}'")),
        None => errors.push(".cursor-plugin/plugin.json is missing name".to_string()),
    }
    if let Some(variables) = value.get("variables") {
        let ok = variables.get("type").and_then(|v| v.as_str()) == Some("object")
            && variables
                .get("properties")
                .and_then(|v| v.as_object())
                .is_some();
        if !ok {
            errors.push(
                "variables must be a JSON Schema object with type \"object\" and properties"
                    .to_string(),
            );
        }
    }
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
            Ok(content) => content,
            Err(err) => {
                errors.push(format!("skills/{dir_name}/SKILL.md unreadable: {err}"));
                continue;
            }
        };
        let Some(fm) = frontmatter_mapping(&content) else {
            errors.push(format!(
                "skills/{dir_name}/SKILL.md has no parseable frontmatter"
            ));
            continue;
        };
        match fm.get("name").and_then(|v| v.as_str()) {
            Some(skill_name) if skill_name == dir_name && name::is_valid_skill_name(skill_name) => {
            }
            Some(skill_name) => errors.push(format!(
                "skills/{dir_name}: name '{skill_name}' must match the folder and be lowercase kebab-case"
            )),
            None => errors.push(format!("skills/{dir_name}: missing name")),
        }
        match fm.get("description").and_then(|v| v.as_str()) {
            Some(desc) if !desc.is_empty() => {}
            _ => errors.push(format!("skills/{dir_name}: missing description")),
        }
    }
}

fn validate_rules(dir: &Path, errors: &mut Vec<String>, warnings: &mut Vec<String>) {
    let rules_dir = dir.join("rules");
    if !rules_dir.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(&rules_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !matches!(ext, "md" | "mdc" | "markdown") {
            warnings.push(format!(
                "rules/{} is not a .md, .mdc, or .markdown file",
                path.file_name().and_then(|n| n.to_str()).unwrap_or("rule")
            ));
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some(fm) = frontmatter_mapping(&content) else {
            errors.push(format!(
                "rules/{} is missing YAML frontmatter",
                file_name(&path)
            ));
            continue;
        };
        if fm.get("description").and_then(|v| v.as_str()).is_none() {
            warnings.push(format!("rules/{}: missing description", file_name(&path)));
        }
    }
}

fn validate_agents(dir: &Path, warnings: &mut Vec<String>) {
    validate_markdown_component(dir, "agents", &["md", "mdc", "markdown"], warnings);
}

fn validate_commands(dir: &Path, warnings: &mut Vec<String>) {
    validate_markdown_component(dir, "commands", &["md", "mdc", "markdown", "txt"], warnings);
}

fn validate_markdown_component(
    dir: &Path,
    folder: &str,
    exts: &[&str],
    warnings: &mut Vec<String>,
) {
    let root = dir.join(folder);
    if !root.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(&root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !exts.contains(&ext) {
            warnings.push(format!(
                "{folder}/{} has an unexpected extension",
                file_name(&path)
            ));
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some(fm) = frontmatter_mapping(&content) else {
            warnings.push(format!(
                "{folder}/{}: missing frontmatter (Cursor expects name and description)",
                file_name(&path)
            ));
            continue;
        };
        if fm.get("name").and_then(|v| v.as_str()).is_none() {
            warnings.push(format!("{folder}/{}: missing name", file_name(&path)));
        }
        if fm.get("description").and_then(|v| v.as_str()).is_none() {
            warnings.push(format!(
                "{folder}/{}: missing description",
                file_name(&path)
            ));
        }
    }
}

fn validate_mcp(dir: &Path, errors: &mut Vec<String>) {
    let path = dir.join("mcp.json");
    if !path.is_file() {
        return;
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) => {
            errors.push(format!("mcp.json unreadable: {err}"));
            return;
        }
    };
    let value: serde_json::Value = match serde_json::from_str(&content) {
        Ok(value) => value,
        Err(err) => {
            errors.push(format!("mcp.json is not valid JSON: {err}"));
            return;
        }
    };
    let Some(servers) = value.get("mcpServers").and_then(|v| v.as_object()) else {
        errors.push("mcp.json is missing mcpServers object".to_string());
        return;
    };
    for (server_name, server) in servers {
        let has_command = server
            .get("command")
            .and_then(|v| v.as_str())
            .is_some_and(|c| !c.is_empty());
        let has_url = server
            .get("url")
            .and_then(|v| v.as_str())
            .is_some_and(|u| !u.is_empty());
        if !has_command && !has_url {
            errors.push(format!(
                "MCP server '{server_name}': needs a command or a url"
            ));
        }
    }
}

fn validate_hooks(dir: &Path, errors: &mut Vec<String>, warnings: &mut Vec<String>) {
    let path = dir.join("hooks").join("hooks.json");
    if !path.is_file() {
        return;
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) => {
            errors.push(format!("hooks/hooks.json unreadable: {err}"));
            return;
        }
    };
    let value: serde_json::Value = match serde_json::from_str(&content) {
        Ok(value) => value,
        Err(err) => {
            errors.push(format!("hooks/hooks.json is not valid JSON: {err}"));
            return;
        }
    };
    let Some(hooks) = value.get("hooks").and_then(|v| v.as_object()) else {
        errors.push("hooks/hooks.json is missing hooks object".to_string());
        return;
    };
    for (event, entries) in hooks {
        if !CURSOR_HOOKS.contains(&event.as_str()) {
            warnings.push(format!(
                "hook event '{event}' is not a documented Cursor hook"
            ));
        }
        let Some(entries) = entries.as_array() else {
            errors.push(format!("hook event '{event}' is not an array"));
            continue;
        };
        for entry in entries {
            let has_command = entry.get("command").and_then(|v| v.as_str()).is_some();
            let has_prompt = entry.get("type").and_then(|v| v.as_str()) == Some("prompt")
                && entry.get("prompt").and_then(|v| v.as_str()).is_some();
            if !has_command && !has_prompt {
                errors.push(format!(
                    "hook event '{event}': entry needs a command or a prompt"
                ));
            }
        }
    }
}

fn frontmatter_mapping(content: &str) -> Option<serde_yaml::Mapping> {
    let content = content.trim_start();
    if !content.starts_with("---") {
        return None;
    }
    let rest = &content[3..];
    let end = rest.find("---")?;
    let value: serde_yaml::Value = serde_yaml::from_str(&rest[..end]).ok()?;
    value.as_mapping().cloned()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_minimal_cursor_plugin() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("plugin");
        std::fs::create_dir_all(dir.join(".cursor-plugin")).unwrap();
        std::fs::create_dir_all(dir.join("skills").join("demo")).unwrap();
        std::fs::write(
            dir.join(".cursor-plugin/plugin.json"),
            r#"{"name":"demo","description":"Demo"}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("skills/demo/SKILL.md"),
            "---\nname: demo\ndescription: Demo skill\n---\n\nBody.\n",
        )
        .unwrap();
        let (errors, warnings) = validate_plugin(&dir);
        assert!(errors.is_empty(), "{errors:?}");
        assert!(warnings.is_empty(), "{warnings:?}");
    }

    #[test]
    fn rejects_skill_name_mismatch() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("plugin");
        std::fs::create_dir_all(dir.join(".cursor-plugin")).unwrap();
        std::fs::create_dir_all(dir.join("skills").join("demo")).unwrap();
        std::fs::write(dir.join(".cursor-plugin/plugin.json"), r#"{"name":"demo"}"#).unwrap();
        std::fs::write(
            dir.join("skills/demo/SKILL.md"),
            "---\nname: other\ndescription: Demo\n---\n\nBody.\n",
        )
        .unwrap();
        let (errors, _) = validate_plugin(&dir);
        assert!(errors.iter().any(|e| e.contains("must match the folder")));
    }
}
