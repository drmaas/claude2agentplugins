use std::collections::HashSet;
use std::path::Path;

use crate::claude::manifest::{ClaudeManifest, StringOrArray};
use crate::error::Result;
use crate::opencode::agents::resolve_inside;
use crate::opencode::markdown::{
    extra_string, extra_value, fit_name, map_model, read_markdown, unhandled_keys,
};

#[derive(Debug, Clone)]
pub struct RegisteredCommand {
    pub name: String,
    pub description: String,
    pub template: String,
}

const HANDLED_EXTRA: &[&str] = &[
    "model",
    "argument-hint",
    "argumentHint",
    "disable-model-invocation",
    "disableModelInvocation",
    "tools",
    "allowedTools",
];

pub fn convert(
    plugin_root: &Path,
    manifest: &ClaudeManifest,
) -> Result<(Vec<RegisteredCommand>, Vec<String>)> {
    let mut warnings = Vec::new();
    let mut commands = Vec::new();
    let mut seen = HashSet::new();

    for path in command_files(plugin_root, manifest, &mut warnings) {
        let markdown = read_markdown(&path)?;
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("command");
        let name = fit_name(stem);
        if name != stem {
            warnings.push(format!(
                "Command '{stem}': renamed to '{name}' to satisfy OpenCode v2 command names"
            ));
        }
        if !seen.insert(name.clone()) {
            warnings.push(format!(
                "Command '{stem}': duplicate command name '{name}'; skipped"
            ));
            continue;
        }
        if markdown.frontmatter_unparsed {
            warnings.push(format!(
                "Command '{name}': frontmatter could not be parsed; the body was used as the template"
            ));
        }

        let frontmatter = markdown.frontmatter.as_ref();
        let mut description = frontmatter
            .and_then(|item| item.description.clone())
            .map(|description| description.trim().to_string())
            .unwrap_or_default();
        if description.is_empty() {
            description = crate::claude::skill::command_description(
                &crate::claude::skill::CommandFile {
                    frontmatter: markdown.frontmatter.clone(),
                    body: markdown.body.clone(),
                    raw: String::new(),
                },
                &name,
            );
            warnings.push(format!(
                "Command '{name}': missing description; synthesized from the command body"
            ));
        }
        if let Some(frontmatter) = frontmatter
            && let Some(hint) = extra_string(frontmatter, &["argument-hint", "argumentHint"])
        {
            warnings.push(format!(
                "Command '{name}': argument-hint '{hint}' has no OpenCode v2 field; appended to the description"
            ));
            description = format!("{description} (args: {hint})");
        }
        if let Some(frontmatter) = frontmatter {
            if frontmatter.allowed_tools.is_some()
                || extra_value(frontmatter, &["tools", "allowedTools"]).is_some()
            {
                warnings.push(format!(
                    "Command '{name}': allowed-tools has no OpenCode v2 command equivalent; not applied"
                ));
            }
            if extra_value(
                frontmatter,
                &["disable-model-invocation", "disableModelInvocation"],
            )
            .is_some()
            {
                warnings.push(format!(
                    "Command '{name}': disable-model-invocation has no OpenCode v2 equivalent; omitted"
                ));
            }
            for key in unhandled_keys(frontmatter, HANDLED_EXTRA) {
                warnings.push(format!(
                    "Command '{name}': frontmatter field '{key}' has no OpenCode v2 equivalent; omitted"
                ));
            }
        }

        if let Some(raw) = frontmatter.and_then(|item| extra_string(item, &["model"]))
            && !raw.trim().is_empty()
            && !raw.eq_ignore_ascii_case("inherit")
        {
            let _ = map_model(&raw, &format!("Command '{name}'"), &mut warnings);
            warnings.push(format!(
                "Command '{name}': OpenCode v2 command registration accepts name, description, and execute; model was omitted"
            ));
        }

        let template = markdown.body.trim().to_string();
        if template.is_empty() {
            warnings.push(format!("Command '{name}': template body is empty"));
        }

        commands.push(RegisteredCommand {
            name,
            description,
            template,
        });
    }

    Ok((commands, warnings))
}

fn command_files(
    plugin_root: &Path,
    manifest: &ClaudeManifest,
    warnings: &mut Vec<String>,
) -> Vec<std::path::PathBuf> {
    match &manifest.commands {
        None => markdown_files(&plugin_root.join("commands"), warnings),
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
        warnings.push(format!("Command path '{raw}' was not found; skipped"));
        return Vec::new();
    };
    if resolved.is_file() {
        return vec![resolved];
    }
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
                "Command directory '{}' contains subdirectory '{}' which was not converted (OpenCode v2 command files are registered individually)",
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
