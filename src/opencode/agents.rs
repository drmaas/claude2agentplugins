use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use crate::claude::manifest::{ClaudeManifest, StringOrArray};
use crate::claude::skill::SkillFrontmatter;
use crate::error::Result;
use crate::opencode::markdown::{
    extra_string, extra_value, fit_name, map_model, read_markdown, string_list, unhandled_keys,
    yaml_u64,
};

#[derive(Debug, Clone)]
pub struct RegisteredAgent {
    pub id: String,
    pub description: String,
    pub system: String,
    pub mode: &'static str,
    pub permissions: Vec<PermissionRule>,
    pub color: Option<String>,
    pub steps: Option<u64>,
    pub model: Option<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct PermissionRule {
    pub action: String,
    pub resource: String,
    pub effect: String,
}

const HANDLED_EXTRA: &[&str] = &[
    "tools",
    "model",
    "permissionMode",
    "permission-mode",
    "maxTurns",
    "max-turns",
    "color",
    "skills",
    "hooks",
    "allowedTools",
];

const BUILTIN_AGENTS: &[&str] = &[
    "build",
    "plan",
    "general",
    "explore",
    "scout",
    "compaction",
    "title",
    "summary",
];

pub fn convert(
    plugin_root: &Path,
    manifest: &ClaudeManifest,
) -> Result<(Vec<RegisteredAgent>, Vec<String>)> {
    let mut warnings = Vec::new();
    let mut agents = Vec::new();
    let mut seen = HashSet::new();

    for path in agent_files(plugin_root, manifest, &mut warnings) {
        let markdown = read_markdown(&path)?;
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("agent");
        let id = fit_name(stem);
        if id != stem {
            warnings.push(format!(
                "Agent '{stem}': renamed to '{id}' to satisfy OpenCode v2 agent ids"
            ));
        }
        if !seen.insert(id.clone()) {
            warnings.push(format!(
                "Agent '{stem}': duplicate agent id '{id}'; skipped"
            ));
            continue;
        }
        if markdown.frontmatter_unparsed {
            warnings.push(format!(
                "Agent '{id}': frontmatter could not be parsed; description synthesized and the original file is kept in the sidecar when present"
            ));
        }
        if BUILTIN_AGENTS.contains(&id.as_str()) {
            warnings.push(format!(
                "Agent '{id}': this id matches an OpenCode built-in agent and editor.update will replace it"
            ));
        }

        let frontmatter = markdown.frontmatter.as_ref();
        let mut description = frontmatter
            .and_then(|item| item.description.clone())
            .unwrap_or_default();
        description = description.trim().to_string();
        if description.is_empty() {
            description = format!("Converted Claude agent {id}");
            warnings.push(format!(
                "Agent '{id}': missing description; synthesized '{description}'"
            ));
        }

        if let Some(frontmatter) = frontmatter {
            if let Some(mode) = extra_string(frontmatter, &["permissionMode", "permission-mode"]) {
                warnings.push(format!(
                    "Agent '{id}': permissionMode '{mode}' has no OpenCode v2 equivalent; omitted"
                ));
            }
            if extra_value(frontmatter, &["skills"]).is_some() {
                warnings.push(format!(
                    "Agent '{id}': skill preload has no OpenCode v2 equivalent; omitted"
                ));
            }
            if extra_value(frontmatter, &["hooks"]).is_some() {
                warnings.push(format!(
                    "Agent '{id}': per-agent hooks have no OpenCode v2 equivalent; omitted"
                ));
            }
            for key in unhandled_keys(frontmatter, HANDLED_EXTRA) {
                warnings.push(format!(
                    "Agent '{id}': frontmatter field '{key}' has no OpenCode v2 equivalent; omitted"
                ));
            }
        }

        let model = frontmatter.and_then(|item| {
            extra_string(item, &["model"])
                .and_then(|model| map_model(&model, &format!("Agent '{id}'"), &mut warnings))
                .and_then(|model| split_model(&model))
        });

        let steps = frontmatter.and_then(|item| steps_from(item, &id, &mut warnings));
        let color = frontmatter.and_then(|item| color_from(item, &id, &mut warnings));
        let permissions = permissions_from(frontmatter, &id, &mut warnings);
        let system = markdown.body.trim().to_string();
        if system.is_empty() {
            warnings.push(format!("Agent '{id}': prompt body is empty"));
        }

        agents.push(RegisteredAgent {
            id,
            description,
            system,
            mode: "subagent",
            permissions,
            color,
            steps,
            model,
        });
    }

    Ok((agents, warnings))
}

fn steps_from(frontmatter: &SkillFrontmatter, id: &str, warnings: &mut Vec<String>) -> Option<u64> {
    let value = extra_value(frontmatter, &["maxTurns", "max-turns"])?;
    match yaml_u64(value) {
        Some(steps) if steps > 0 => {
            warnings.push(format!(
                "Agent '{id}': maxTurns mapped to OpenCode v2 steps"
            ));
            Some(steps)
        }
        Some(_) => {
            warnings.push(format!(
                "Agent '{id}': maxTurns must be a positive integer; omitted"
            ));
            None
        }
        None => {
            warnings.push(format!("Agent '{id}': maxTurns is not an integer; omitted"));
            None
        }
    }
}

fn color_from(
    frontmatter: &SkillFrontmatter,
    id: &str,
    warnings: &mut Vec<String>,
) -> Option<String> {
    let color = extra_string(frontmatter, &["color"])?;
    if is_opencode_color(&color) {
        Some(color)
    } else {
        warnings.push(format!(
            "Agent '{id}': color '{color}' is not an OpenCode hex color or theme color; omitted"
        ));
        None
    }
}

fn is_opencode_color(color: &str) -> bool {
    const THEME: &[&str] = &[
        "primary",
        "secondary",
        "accent",
        "success",
        "warning",
        "error",
        "info",
    ];
    if THEME.contains(&color) {
        return true;
    }
    let Some(hex) = color.strip_prefix('#') else {
        return false;
    };
    hex.len() == 6 && hex.chars().all(|character| character.is_ascii_hexdigit())
}

fn permissions_from(
    frontmatter: Option<&SkillFrontmatter>,
    id: &str,
    warnings: &mut Vec<String>,
) -> Vec<PermissionRule> {
    let Some(frontmatter) = frontmatter else {
        return allow_all();
    };
    let tools = if let Some(value) = extra_value(frontmatter, &["tools"]) {
        if frontmatter.allowed_tools.is_some() {
            warnings.push(format!(
                "Agent '{id}': allowed-tools ignored because tools is set"
            ));
        }
        string_list(value)
    } else if let Some(allowed) = &frontmatter.allowed_tools {
        allowed
            .to_agent_string()
            .split_whitespace()
            .map(str::to_string)
            .collect()
    } else {
        return allow_all();
    };

    if tools
        .iter()
        .any(|tool| tool == "*" || tool.eq_ignore_ascii_case("all"))
    {
        warnings.push(format!(
            "Agent '{id}': tools allowlist is unrestricted; OpenCode permissions allow every action"
        ));
        return allow_all();
    }

    let mut allowed = BTreeMap::<String, ()>::new();
    for tool in &tools {
        if tool.contains('(') {
            warnings.push(format!(
                "Agent '{id}': tool pattern '{tool}' mapped by tool name only; command patterns were not translated"
            ));
        }
        let base = tool
            .split(['(', ':'])
            .next()
            .unwrap_or(tool)
            .trim()
            .to_ascii_lowercase();
        match base.as_str() {
            "read" | "readfile" => {
                allowed.insert("read".to_string(), ());
            }
            "write" | "edit" | "multiedit" | "notebookedit" => {
                allowed.insert("edit".to_string(), ());
            }
            "glob" => {
                allowed.insert("glob".to_string(), ());
            }
            "grep" => {
                allowed.insert("grep".to_string(), ());
            }
            "ls" | "list" | "listdir" => {
                allowed.insert("list".to_string(), ());
            }
            "bash" | "shell" => {
                allowed.insert("bash".to_string(), ());
            }
            "task" | "agent" => {
                allowed.insert("task".to_string(), ());
            }
            "todowrite" | "todoread" => {
                allowed.insert("todowrite".to_string(), ());
            }
            "webfetch" => {
                allowed.insert("webfetch".to_string(), ());
            }
            "websearch" => {
                allowed.insert("websearch".to_string(), ());
            }
            "skill" => {
                allowed.insert("skill".to_string(), ());
            }
            "lsp" => {
                allowed.insert("lsp".to_string(), ());
            }
            "question" => {
                allowed.insert("question".to_string(), ());
            }
            _ => warnings.push(format!(
                "Agent '{id}': tool '{tool}' has no OpenCode v2 permission action; denied"
            )),
        }
    }

    warnings.push(format!(
        "Agent '{id}': tools allowlist mapped to OpenCode v2 permission rules; tools not listed are denied"
    ));
    let mut rules = vec![PermissionRule {
        action: "*".to_string(),
        resource: "*".to_string(),
        effect: "deny".to_string(),
    }];
    for action in allowed.keys() {
        rules.push(PermissionRule {
            action: action.clone(),
            resource: "*".to_string(),
            effect: "allow".to_string(),
        });
    }
    rules
}

fn allow_all() -> Vec<PermissionRule> {
    vec![PermissionRule {
        action: "*".to_string(),
        resource: "*".to_string(),
        effect: "allow".to_string(),
    }]
}

fn split_model(model: &str) -> Option<(String, String)> {
    let (provider, id) = model.split_once('/')?;
    if provider.is_empty() || id.is_empty() {
        None
    } else {
        Some((provider.to_string(), id.to_string()))
    }
}

fn agent_files(
    plugin_root: &Path,
    manifest: &ClaudeManifest,
    warnings: &mut Vec<String>,
) -> Vec<std::path::PathBuf> {
    match &manifest.agents {
        None => markdown_files(&plugin_root.join("agents"), warnings),
        Some(StringOrArray::Single(path)) => files_from_spec(plugin_root, path, warnings),
        Some(StringOrArray::Multiple(paths)) => {
            let mut files = Vec::new();
            for path in paths {
                files.extend(files_from_spec(plugin_root, path, warnings));
            }
            files
        }
    }
}

fn files_from_spec(
    plugin_root: &Path,
    raw: &str,
    warnings: &mut Vec<String>,
) -> Vec<std::path::PathBuf> {
    let Some(resolved) = resolve_inside(plugin_root, raw) else {
        warnings.push(format!("Agent path '{raw}' was not found; skipped"));
        return Vec::new();
    };
    if resolved.is_file() {
        return vec![resolved];
    }
    warnings.push(format!(
        "Agent path '{raw}' is a directory; Claude expects markdown files. Markdown files inside it were converted"
    ));
    markdown_files(&resolved, warnings)
}

fn markdown_files(dir: &Path, warnings: &mut Vec<String>) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return files;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            warnings.push(format!(
                "Agent directory '{}' contains subdirectory '{}' which was not converted",
                dir.display(),
                entry.file_name().to_string_lossy()
            ));
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) == Some("md") {
            files.push(path);
        }
    }
    files
}

pub(crate) fn resolve_inside(plugin_root: &Path, raw: &str) -> Option<std::path::PathBuf> {
    if raw.contains("..") || raw.contains("://") {
        return None;
    }
    let cleaned = raw.trim_start_matches("./").trim_start_matches('/');
    if cleaned.is_empty() {
        return None;
    }
    let resolved = plugin_root.join(cleaned);
    if resolved.starts_with(plugin_root) && resolved.exists() {
        Some(resolved)
    } else {
        None
    }
}
