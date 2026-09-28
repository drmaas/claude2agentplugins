use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::cursor::markdown::{self, insert_string};
use crate::error::Result;
use crate::validate::name;

pub struct WrittenRule {
    pub name: String,
    pub warnings: Vec<String>,
}

pub fn convert(input: &Path, output: &Path) -> Result<Vec<WrittenRule>> {
    let mut written = Vec::new();
    let mut used = HashSet::new();
    let mut sources = rule_files(input);
    if let Some(claude_md) = claude_md(input) {
        sources.push(claude_md);
    }
    for path in sources {
        written.push(write_rule(input, &path, output, &mut used)?);
    }
    Ok(written)
}

fn claude_md(input: &Path) -> Option<PathBuf> {
    let path = input.join("CLAUDE.md");
    path.is_file().then_some(path)
}

fn rule_files(input: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for root in [input.join("rules"), input.join(".claude").join("rules")] {
        walk_rules(&root, &mut files);
    }
    if let Some(extra) = manifest_rule_paths(input) {
        for raw in extra {
            let cleaned = raw.trim_start_matches("./").trim_start_matches('/');
            if cleaned.is_empty() {
                continue;
            }
            let resolved = input.join(cleaned);
            if resolved.is_file() {
                files.push(resolved);
            } else if resolved.is_dir() {
                walk_rules(&resolved, &mut files);
            }
        }
    }
    files.sort();
    files.dedup();
    files
}

fn walk_rules(dir: &Path, files: &mut Vec<PathBuf>) {
    if !dir.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<_> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            walk_rules(&path, files);
        } else if is_rule_file(&path) {
            files.push(path);
        }
    }
}

fn is_rule_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("md" | "mdc" | "markdown")
    )
}

fn manifest_rule_paths(input: &Path) -> Option<Vec<String>> {
    let path = input.join(".claude-plugin").join("plugin.json");
    let content = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&content).ok()?;
    match value.get("rules")? {
        serde_json::Value::String(s) => Some(vec![s.clone()]),
        serde_json::Value::Array(items) => Some(
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
        ),
        _ => None,
    }
}

fn write_rule(
    input: &Path,
    path: &Path,
    output: &Path,
    used: &mut HashSet<String>,
) -> Result<WrittenRule> {
    let raw = std::fs::read_to_string(path)?;
    let doc = markdown::parse(&raw);
    let mut warnings = Vec::new();
    let is_claude_md = path.file_name().and_then(|n| n.to_str()) == Some("CLAUDE.md");
    let stem = if is_claude_md {
        "claude".to_string()
    } else {
        path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("rule")
            .to_string()
    };
    let mut normalized = name::normalize_skill_name(&stem);
    if !used.insert(normalized.clone()) {
        let base = normalized.clone();
        let mut n = 2;
        loop {
            normalized = format!("{base}-{n}");
            if used.insert(normalized.clone()) {
                break;
            }
            n += 1;
        }
        warnings.push(format!(
            "Rule '{stem}': renamed to '{normalized}' to avoid a file name collision"
        ));
    }

    let mut globs = doc.frontmatter.as_ref().and_then(read_globs);
    let mut description = doc
        .frontmatter
        .as_ref()
        .and_then(|fm| markdown::mapping_string(fm, "description"))
        .filter(|d| !d.trim().is_empty());
    let explicit_always = doc
        .frontmatter
        .as_ref()
        .and_then(|fm| markdown::mapping_get(fm, "alwaysApply"))
        .and_then(markdown::yaml_bool);

    if is_claude_md {
        warnings.push(
            "CLAUDE.md mapped to rules/claude.mdc with alwaysApply: true (Claude loads it every session; Cursor rules are the plugin equivalent)"
                .to_string(),
        );
        if description.is_none() {
            description = Some("Instructions converted from CLAUDE.md".to_string());
        }
    } else if globs.is_some() {
        warnings.push(format!(
            "Rule '{stem}': Claude paths/globs mapped to Cursor rule globs"
        ));
    }

    if description.is_none() {
        description = Some(format!("Converted Claude rule '{normalized}'."));
        warnings.push(format!(
            "Rule '{stem}': missing description; synthesized one"
        ));
    }

    if let Some(fm) = &doc.frontmatter
        && let Some(map) = fm.as_mapping()
    {
        for key in map.keys() {
            let Some(key) = key.as_str() else { continue };
            if matches!(key, "description" | "alwaysApply" | "globs" | "paths") {
                continue;
            }
            warnings.push(format!(
                "Rule '{stem}': frontmatter '{key}' has no Cursor rule equivalent; dropped"
            ));
        }
    }

    let mut front = serde_yaml::Mapping::new();
    insert_string(&mut front, "description", description.unwrap_or_default());
    match (explicit_always, globs.is_some(), is_claude_md) {
        (Some(value), _, _) => {
            front.insert(
                serde_yaml::Value::String("alwaysApply".to_string()),
                serde_yaml::Value::Bool(value),
            );
        }
        (None, false, _) => {
            front.insert(
                serde_yaml::Value::String("alwaysApply".to_string()),
                serde_yaml::Value::Bool(true),
            );
        }
        (None, true, _) => {}
    }
    if let Some(globs) = globs.take() {
        front.insert(serde_yaml::Value::String("globs".to_string()), globs);
    }

    let rel = path
        .strip_prefix(input)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string());
    let nested = path
        .parent()
        .is_some_and(|parent| parent != input.join("rules") && parent != input);
    if !is_claude_md && nested {
        warnings.push(format!(
            "Rule '{rel}': nested path flattened to rules/{normalized}.mdc"
        ));
    }

    markdown::write(
        &output.join("rules").join(format!("{normalized}.mdc")),
        &serde_yaml::Value::Mapping(front),
        &doc.body,
    )?;
    Ok(WrittenRule {
        name: normalized,
        warnings,
    })
}

fn read_globs(fm: &serde_yaml::Value) -> Option<serde_yaml::Value> {
    let value =
        markdown::mapping_get(fm, "globs").or_else(|| markdown::mapping_get(fm, "paths"))?;
    match value {
        serde_yaml::Value::String(s) if !s.trim().is_empty() => Some(value.clone()),
        serde_yaml::Value::Sequence(items) if !items.is_empty() => {
            Some(serde_yaml::Value::Sequence(items.clone()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_paths_to_globs_and_claude_md() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("rules")).unwrap();
        std::fs::write(
            temp.path().join("rules/prefer-const.md"),
            "---\ndescription: Prefer const\npaths:\n  - \"**/*.ts\"\n---\n\nUse const.\n",
        )
        .unwrap();
        std::fs::write(
            temp.path().join("CLAUDE.md"),
            "# Style\n\nFollow the house style.\n",
        )
        .unwrap();
        let output = temp.path().join("out");
        let written = convert(temp.path(), &output).unwrap();
        assert_eq!(written.len(), 2);
        let rule = std::fs::read_to_string(output.join("rules/prefer-const.mdc")).unwrap();
        assert!(rule.contains("**/*.ts"));
        assert!(rule.contains("globs:"));
        assert!(!rule.contains("paths:"));
        let claude = std::fs::read_to_string(output.join("rules/claude.mdc")).unwrap();
        assert!(claude.contains("alwaysApply: true"));
        assert!(claude.contains("Follow the house style."));
    }
}
