pub mod agents;
pub mod commands;
pub mod hooks;
pub mod markdown;
pub mod marketplace;
pub mod mcp;
pub mod render;
pub mod sidecar;
pub mod skills;
pub mod validate;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::claude::manifest::{ClaudeManifest, StringOrArray};
use crate::error::{Error, Result};
use crate::validate::name;

#[derive(Debug, Clone)]
pub struct OpenCodeOptions {
    pub extension_namespace: String,
    pub strict: bool,
    pub force: bool,
    pub preferred_name: Option<String>,
}

impl Default for OpenCodeOptions {
    fn default() -> Self {
        Self {
            extension_namespace: "com.claude.code".to_string(),
            strict: false,
            force: false,
            preferred_name: None,
        }
    }
}

#[derive(Debug)]
pub struct OpenCodeReport {
    pub name: String,
    pub output: PathBuf,
    pub manifest_synthesized: bool,
    pub skills_converted: usize,
    pub agents_converted: usize,
    pub commands_converted: usize,
    pub mcp_servers: usize,
    pub hooks_mapped: usize,
    pub sidecar_entries: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Default)]
pub struct OpenCodeDirectoryReport {
    pub plugins: Vec<OpenCodeReport>,
    pub skipped: Vec<String>,
}

pub fn convert_single(
    input: &Path,
    output: &Path,
    options: &OpenCodeOptions,
) -> Result<OpenCodeReport> {
    if !input.is_dir() {
        return Err(Error::Conversion(format!(
            "Input '{}' is not a directory",
            input.display()
        )));
    }
    if output.exists() && !options.force && std::fs::read_dir(output)?.next().is_some() {
        return Err(Error::Conversion(format!(
            "Output directory '{}' is not empty; use --force to overwrite",
            output.display()
        )));
    }

    let dir_name = input
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("plugin")
        .to_string();
    let synth_name = options
        .preferred_name
        .clone()
        .unwrap_or_else(|| dir_name.clone());
    let (manifest, manifest_synthesized) = match crate::claude::manifest::parse(input) {
        Ok(manifest) => (manifest, false),
        Err(Error::NotFound { .. }) => (ClaudeManifest::synthesize(&synth_name), true),
        Err(err) => return Err(err),
    };

    let mut warnings = Vec::new();
    if manifest_synthesized {
        warnings.push(format!(
            "No .claude-plugin/plugin.json found; synthesized manifest with name '{}'",
            manifest.name
        ));
    }

    let plugin_id = name::normalize_name(&manifest.name);
    let package_name = package_name(&plugin_id);
    let version = package_version(manifest.version.as_deref(), &plugin_id, &mut warnings);

    let settings = crate::claude::settings::load(input, &manifest)?;
    let default_agent = settings.agent.as_deref();
    if settings.subagent_status_line.is_some() {
        let source = if settings.from_file {
            "settings.json"
        } else {
            "plugin.json settings"
        };
        warnings.push(format!(
            "{source} key 'subagentStatusLine' has no OpenCode v2 equivalent; preserved under extensions/"
        ));
    }
    for key in &settings.unknown_keys {
        let source = if settings.from_file {
            "settings.json"
        } else {
            "plugin.json settings"
        };
        warnings.push(format!(
            "{source} key '{key}' is ignored by Claude Code and has no OpenCode v2 equivalent; preserved under extensions/"
        ));
    }

    let (claude_skills, skill_path_warnings) = load_skills(input, &manifest, &plugin_id)?;
    warnings.extend(skill_path_warnings);
    let (registered_skills, skill_warnings) = skills::convert(input, output, &claude_skills)?;
    warnings.extend(skill_warnings);

    let (agents, agent_warnings) = agents::convert(input, &manifest, default_agent)?;
    warnings.extend(agent_warnings);
    let (commands, command_warnings) = commands::convert(input, &manifest)?;
    warnings.extend(command_warnings);
    let mut converted_hooks = hooks::convert(input, &manifest)?;
    warnings.append(&mut converted_hooks.warnings);

    let loaded_mcp = mcp::load(input, &manifest);
    warnings.extend(loaded_mcp.warnings);
    let mut converted_mcp = if loaded_mcp.present {
        mcp::convert(&loaded_mcp.config)
    } else {
        mcp::ConvertedMcp {
            servers: std::collections::BTreeMap::new(),
            warnings: Vec::new(),
            referenced_roots: Vec::new(),
        }
    };
    warnings.append(&mut converted_mcp.warnings);

    std::fs::create_dir_all(output)?;
    let mut root_kept = copy_referenced(
        input,
        output,
        &converted_mcp.referenced_roots,
        &mut warnings,
    )?;
    let has_bin = input.join("bin").is_dir();
    if has_bin {
        crate::convert::extensions::copy_entry(&input.join("bin"), &output.join("bin"))?;
        warnings.push(
            "bin/ copied into the OpenCode package and prepended to shell PATH via ctx.shell.hook(\"create.before\") (Claude Bash PATH approximation)"
                .to_string(),
        );
    }
    if (converted_hooks.needs_scripts || !converted_hooks.mapped.is_empty())
        && hooks::copy_scripts(input, output)?
    {
        root_kept.insert("scripts".to_string());
        warnings.push(
            "scripts/ was copied into the OpenCode v2 plugin package so mapped Claude command hooks can run"
                .to_string(),
        );
    }
    render::write_package(output, &manifest, &package_name, &version)?;
    render::write_plugin(
        output,
        &plugin_id,
        &render::PluginParts {
            skills: &registered_skills,
            agents: &agents,
            commands: &commands,
            servers: &converted_mcp.servers,
            hooks: &converted_hooks.mapped,
            prepend_bin_path: has_bin,
        },
    )?;

    let sidecar = sidecar::write(
        input,
        output,
        &manifest,
        &options.extension_namespace,
        &root_kept,
        !converted_hooks.mapped.is_empty(),
    )?;
    warnings.extend(sidecar.warnings);
    copy_root_docs(input, output)?;

    let (validation_errors, validation_warnings) = validate::validate(output);
    warnings.extend(validation_errors);
    warnings.extend(validation_warnings);

    if options.strict && !warnings.is_empty() {
        return Err(Error::Conversion(format!(
            "Conversion produced {} warning(s) and --strict is enabled",
            warnings.len()
        )));
    }

    Ok(OpenCodeReport {
        name: manifest.name,
        output: output.to_path_buf(),
        manifest_synthesized,
        skills_converted: registered_skills.len(),
        agents_converted: agents.len(),
        commands_converted: commands.len(),
        mcp_servers: converted_mcp.servers.len(),
        hooks_mapped: converted_hooks.mapped.len(),
        sidecar_entries: sidecar.entries,
        warnings,
    })
}

pub fn convert_directory(
    input: &Path,
    output: &Path,
    options: &OpenCodeOptions,
) -> Result<OpenCodeDirectoryReport> {
    let mut report = OpenCodeDirectoryReport::default();
    let mut used_names: HashMap<String, PathBuf> = HashMap::new();

    for entry in std::fs::read_dir(input)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let is_plugin = path.join(".claude-plugin").join("plugin.json").is_file()
            || path.join("skills").is_dir()
            || path.join("SKILL.md").is_file();
        if !is_plugin {
            report.skipped.push(format!(
                "{} (no plugin.json, skills/, or SKILL.md)",
                path.display()
            ));
            continue;
        }
        let dir_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unknown");
        let normalized_name = name::normalize_name(dir_name);
        if let Some(previous) = used_names.get(&normalized_name) {
            return Err(Error::Conversion(format!(
                "Name collision: '{}' and '{}' both normalize to '{normalized_name}'",
                previous.display(),
                path.display()
            )));
        }
        used_names.insert(normalized_name.clone(), path.clone());
        let plugin_report = convert_single(&path, &output.join(&normalized_name), options)?;
        report.plugins.push(plugin_report);
    }

    if report.plugins.is_empty() {
        return Err(Error::NotFound {
            path: input.display().to_string(),
            reason: "No plugins found (no .claude-plugin/plugin.json, skills/, or SKILL.md)"
                .to_string(),
        });
    }
    Ok(report)
}

fn package_name(plugin_id: &str) -> String {
    if plugin_id.starts_with("opencode-") {
        plugin_id.to_string()
    } else {
        format!("opencode-{plugin_id}")
    }
}

fn package_version(version: Option<&str>, plugin_id: &str, warnings: &mut Vec<String>) -> String {
    match version {
        Some(version)
            if version.split('.').count() == 3
                && version.split('.').all(|part| {
                    !part.is_empty() && part.chars().all(|character| character.is_ascii_digit())
                }) =>
        {
            version.to_string()
        }
        Some(version) => {
            warnings.push(format!(
                "Plugin '{plugin_id}': version '{version}' is not numeric semver; package.json version set to 0.0.0"
            ));
            "0.0.0".to_string()
        }
        None => {
            warnings.push(format!(
                "Plugin '{plugin_id}': missing version; package.json version set to 0.0.0"
            ));
            "0.0.0".to_string()
        }
    }
}

fn load_skills(
    input: &Path,
    manifest: &ClaudeManifest,
    plugin_name: &str,
) -> Result<(Vec<crate::claude::skill::Skill>, Vec<String>)> {
    let mut extra_dirs = Vec::new();
    let mut direct_roots = Vec::new();
    let mut warnings = Vec::new();
    if let Some(skills) = &manifest.skills {
        collect_skill_paths(
            input,
            skills,
            &mut extra_dirs,
            &mut direct_roots,
            &mut warnings,
        );
    }
    let mut skills = crate::claude::skill::parse_all_with_dirs(input, &extra_dirs)?;
    for root in direct_roots {
        let fallback = root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(plugin_name)
            .to_string();
        if let Some(skill) = crate::claude::skill::parse_root_skill(&root, &fallback)? {
            skills.push(skill);
        }
    }
    if skills.is_empty()
        && manifest.skills.is_none()
        && let Some(skill) = crate::claude::skill::parse_root_skill(input, plugin_name)?
    {
        skills.push(skill);
    }
    Ok((skills, warnings))
}

fn collect_skill_paths(
    input: &Path,
    skills: &StringOrArray,
    dirs: &mut Vec<PathBuf>,
    direct_roots: &mut Vec<PathBuf>,
    warnings: &mut Vec<String>,
) {
    let mut visit = |raw: &str| {
        let cleaned = raw.trim_start_matches("./").trim_start_matches('/');
        if cleaned.is_empty() || cleaned == "." {
            if input.join("SKILL.md").is_file() {
                direct_roots.push(input.to_path_buf());
            }
            return;
        }
        if cleaned.contains("..") {
            warnings.push(format!("Skill path '{raw}' escapes the plugin; skipped"));
            return;
        }
        let resolved = input.join(cleaned);
        if !resolved.exists() {
            warnings.push(format!("Skill path '{raw}' was not found; skipped"));
            return;
        }
        if resolved.join("SKILL.md").is_file() {
            direct_roots.push(resolved);
        } else {
            dirs.push(resolved);
        }
    };
    match skills {
        StringOrArray::Single(path) => visit(path),
        StringOrArray::Multiple(paths) => {
            for path in paths {
                visit(path);
            }
        }
    }
}

fn copy_referenced(
    input: &Path,
    output: &Path,
    roots: &[String],
    warnings: &mut Vec<String>,
) -> Result<HashSet<String>> {
    let mut kept = HashSet::new();
    for root in roots {
        if is_reserved_root(root) {
            warnings.push(format!(
                "MCP path references '{root}', which is not copied to the plugin package root"
            ));
            continue;
        }
        let source = input.join(root);
        if !source.exists() {
            warnings.push(format!(
                "MCP path references '{root}' but it was not found in the Claude plugin"
            ));
            continue;
        }
        crate::convert::extensions::copy_entry(&source, &output.join(root))?;
        kept.insert(root.clone());
        warnings.push(format!(
            "'{root}' is referenced by an MCP server and was copied into the OpenCode v2 plugin package"
        ));
    }
    Ok(kept)
}

fn is_reserved_root(name: &str) -> bool {
    matches!(
        name,
        "skills"
            | "commands"
            | "agents"
            | "hooks"
            | ".claude-plugin"
            | ".opencode"
            | "extensions"
            | ".git"
            | ".mcp.json"
            | "src"
    )
}

fn copy_root_docs(input: &Path, output: &Path) -> Result<()> {
    for file in ["LICENSE", "LICENSE.md", "README.md", "CHANGELOG.md"] {
        let source = input.join(file);
        if source.is_file() {
            std::fs::copy(&source, output.join(file))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_manifest_less_skill_plugin() {
        let temp = tempfile::tempdir().unwrap();
        let plugin = temp.path().join("my-tool");
        std::fs::create_dir_all(plugin.join("skills").join("demo")).unwrap();
        std::fs::write(
            plugin.join("skills").join("demo").join("SKILL.md"),
            "---\nname: demo\ndescription: Demo skill\n---\n\nBody.",
        )
        .unwrap();

        let output = temp.path().join("out");
        let report = convert_single(&plugin, &output, &OpenCodeOptions::default()).unwrap();
        assert!(report.manifest_synthesized);
        assert_eq!(report.skills_converted, 1);
        assert!(output.join("package.json").is_file());
        assert!(output.join("src/index.ts").is_file());
        let source = std::fs::read_to_string(output.join("src/index.ts")).unwrap();
        assert!(source.contains("Plugin.define"));
        assert!(source.contains("editor.add"));
        assert!(!output.join("opencode.json").exists());
    }

    #[test]
    fn refuses_non_empty_output_without_force() {
        let temp = tempfile::tempdir().unwrap();
        let plugin = temp.path().join("solo");
        std::fs::create_dir_all(&plugin).unwrap();
        std::fs::write(
            plugin.join("SKILL.md"),
            "---\nname: solo\ndescription: Single skill\n---\n\nBody.",
        )
        .unwrap();
        let output = temp.path().join("out");
        std::fs::create_dir_all(&output).unwrap();
        std::fs::write(output.join("stale.txt"), "old").unwrap();
        assert!(convert_single(&plugin, &output, &OpenCodeOptions::default()).is_err());
        let options = OpenCodeOptions {
            force: true,
            ..OpenCodeOptions::default()
        };
        assert!(convert_single(&plugin, &output, &options).is_ok());
    }
}
