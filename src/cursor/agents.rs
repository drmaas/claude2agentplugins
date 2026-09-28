use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use crate::claude::manifest::{ClaudeManifest, StringOrArray};
use crate::cursor::markdown::{self, insert_string};
use crate::cursor::placeholders;
use crate::error::Result;
use crate::validate::name;

const KNOWN: &[&str] = &["name", "description"];

const CLAUDE_ONLY: &[&str] = &[
    "model",
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
];

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

    if let Some(fm) = &doc.frontmatter
        && let Some(map) = fm.as_mapping()
    {
        for (key, value) in map {
            let Some(key) = key.as_str() else { continue };
            if KNOWN.contains(&key) {
                continue;
            }
            let label = if CLAUDE_ONLY.contains(&key) {
                "Claude-only"
            } else {
                "unmapped"
            };
            warnings.push(format!(
                "Agent '{stem}': {label} frontmatter '{key}' is not part of Cursor's agent schema (name, description); preserved in the sidecar"
            ));
            if let Ok(json) = serde_json::to_value(value) {
                extras.insert(key.to_string(), json);
            }
        }
    }

    let body = placeholders::rewrite_placeholders(&doc.body, &format!("Agent '{stem}'"));
    warnings.extend(body.warnings);

    let mut front = serde_yaml::Mapping::new();
    insert_string(&mut front, "name", normalized.clone());
    insert_string(&mut front, "description", description);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_model_and_keeps_prompt() {
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
        assert_eq!(written.len(), 1);
        let content = std::fs::read_to_string(output.join("agents/security-reviewer.md")).unwrap();
        assert!(content.contains("name: security-reviewer"));
        assert!(content.contains("Be careful."));
        assert!(!content.contains("sonnet"));
        assert!(written[0].warnings.iter().any(|w| w.contains("model")));
        assert_eq!(
            written[0].extras.get("model").and_then(|v| v.as_str()),
            Some("sonnet")
        );
    }
}
