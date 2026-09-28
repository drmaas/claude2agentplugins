pub mod env_vars;
pub mod extensions;
pub mod manifest;
pub mod mcp;
pub mod skills;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::claude;
use crate::claude::manifest::StringOrArray;
use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, clap::ValueEnum)]
pub enum Target {
    #[default]
    #[value(name = "agent-plugins")]
    AgentPlugins,
    #[value(name = "cursor")]
    Cursor,
}

impl Target {
    pub fn as_str(self) -> &'static str {
        match self {
            Target::AgentPlugins => "agent-plugins",
            Target::Cursor => "cursor",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConvertOptions {
    pub extension_namespace: String,
    pub strict: bool,
    pub convert_commands: bool,
    pub force: bool,
    /// Name used when synthesizing a manifest for manifest-less plugins
    /// (e.g. the marketplace entry name, which is the user-facing identifier).
    pub preferred_name: Option<String>,
    pub target: Target,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            extension_namespace: "com.claude.code".to_string(),
            strict: false,
            convert_commands: false,
            force: false,
            preferred_name: None,
            target: Target::AgentPlugins,
        }
    }
}

#[derive(Debug)]
pub struct ConversionReport {
    pub name: String,
    pub output: PathBuf,
    pub manifest_synthesized: bool,
    pub skills_converted: usize,
    pub commands_converted: usize,
    pub mcp_servers: usize,
    pub extension_dirs: usize,
    pub warnings: Vec<String>,
    pub target: Target,
}

#[derive(Debug, Default)]
pub struct DirectoryReport {
    pub plugins: Vec<ConversionReport>,
    pub skipped: Vec<String>,
}

pub fn convert_single(
    input: &Path,
    output: &Path,
    options: &ConvertOptions,
) -> Result<ConversionReport> {
    if options.target == Target::Cursor {
        return crate::cursor::convert_single(input, output, options);
    }

    if output.exists() && !options.force && std::fs::read_dir(output)?.next().is_some() {
        return Err(Error::Conversion(format!(
            "Output directory '{}' is not empty; use --force to overwrite",
            output.display()
        )));
    }

    let dir_name = input
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("plugin")
        .to_string();
    let plugin_name = options
        .preferred_name
        .clone()
        .unwrap_or_else(|| dir_name.clone());

    let (claude_plugin, manifest_synthesized) = match claude::manifest::parse(input) {
        Ok(m) => (m, false),
        Err(Error::NotFound { .. }) => (
            claude::manifest::ClaudeManifest::synthesize(&plugin_name),
            true,
        ),
        Err(e) => return Err(e),
    };

    let mut warnings = Vec::new();
    if manifest_synthesized {
        warnings.push(format!(
            "No .claude-plugin/plugin.json found; synthesized manifest with name '{}'",
            plugin_name
        ));
    }

    let settings = claude::settings::load(input, &claude_plugin)?;
    claude::settings::warn_unmapped(&settings, "Agent Plugins", &mut warnings);

    let mut extra_skill_dirs: Vec<PathBuf> = Vec::new();
    let mut direct_skill_roots: Vec<PathBuf> = Vec::new();
    if let Some(skills) = &claude_plugin.skills {
        collect_skill_paths(
            input,
            skills,
            &mut extra_skill_dirs,
            &mut direct_skill_roots,
        );
    }

    let mut claude_skills = claude::skill::parse_all_with_dirs(input, &extra_skill_dirs)?;
    for root in direct_skill_roots {
        let fallback = root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&plugin_name)
            .to_string();
        if let Some(skill) = claude::skill::parse_root_skill(&root, &fallback)? {
            claude_skills.push(skill);
        }
    }
    if claude_skills.is_empty()
        && claude_plugin.skills.is_none()
        && let Some(skill) = claude::skill::parse_root_skill(input, &plugin_name)?
    {
        claude_skills.push(skill);
    }

    std::fs::create_dir_all(output)?;

    let mut mcp_servers = 0;
    let mcp_path = input.join(".mcp.json");
    if mcp_path.exists() {
        match claude::mcp::parse(input) {
            Ok(mcp_config) => {
                warnings.extend(crate::validate::mcp::validate_mcp_config(&mcp_config));
                let (agent_mcp, mcp_warnings) = mcp::convert(&mcp_config);
                warnings.extend(mcp_warnings);
                crate::agent_plugins::mcp::write(&agent_mcp, output)?;
                mcp_servers = agent_mcp.mcp_servers.len();
            }
            Err(e) => warnings.push(format!(
                "Failed to parse .mcp.json: {e} (original file preserved under the extension namespace)"
            )),
        }
    }

    let agent_manifest = manifest::convert(&claude_plugin, &options.extension_namespace);
    crate::agent_plugins::manifest::write(&agent_manifest, output)?;

    let converted_skills = skills::convert(input, output, &claude_skills)?;
    let skills_count = converted_skills.len();
    for cs in &converted_skills {
        warnings.extend(cs.warnings.clone());
    }

    let mut commands_converted = 0;
    if options.convert_commands {
        let (count, cmd_warnings) = skills::convert_commands(input, output, &claude_plugin)?;
        commands_converted = count;
        warnings.extend(cmd_warnings);
    }

    let extension_entries =
        extensions::collect(input, &claude_plugin, &options.extension_namespace);
    let extension_count = extension_entries.len();
    extensions::write(&extension_entries, output)?;
    if input.join("bin").is_dir() {
        warnings.push(format!(
            "bin/ has no Agent Plugins PATH API; preserved under {}/bin (Claude prepends bin/ to the Bash tool PATH)",
            options.extension_namespace
        ));
    }

    copy_root_docs(input, output)?;

    let (self_errors, self_warnings) = crate::validate::plugin::validate_plugin(output);
    warnings.extend(self_errors);
    warnings.extend(self_warnings);

    if options.strict && !warnings.is_empty() {
        return Err(Error::Conversion(format!(
            "Conversion produced {} warning(s) and --strict is enabled",
            warnings.len()
        )));
    }

    Ok(ConversionReport {
        name: agent_manifest.name.clone(),
        output: output.to_path_buf(),
        manifest_synthesized,
        skills_converted: skills_count,
        commands_converted,
        mcp_servers,
        extension_dirs: extension_count,
        warnings,
        target: Target::AgentPlugins,
    })
}

pub fn convert_directory(
    input: &Path,
    output: &Path,
    options: &ConvertOptions,
) -> Result<DirectoryReport> {
    let mut report = DirectoryReport::default();
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
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        let normalized_name = crate::validate::name::normalize_name(name);
        if let Some(prev) = used_names.get(&normalized_name) {
            return Err(Error::Conversion(format!(
                "Name collision: '{}' and '{}' both normalize to '{}'",
                prev.display(),
                path.display(),
                normalized_name
            )));
        }
        used_names.insert(normalized_name.clone(), path.clone());
        let plugin_output = output.join(&normalized_name);
        let plugin_report = convert_single(&path, &plugin_output, options)?;
        report.plugins.push(plugin_report);
    }

    if report.plugins.is_empty() {
        return Err(Error::NotFound {
            path: input.display().to_string(),
            reason: "No plugins found (no .claude-plugin/plugin.json, skills/, or SKILL.md)"
                .to_string(),
        });
    }
    if options.target == Target::Cursor {
        crate::cursor::marketplace_manifest::write_from_reports(output, &report)?;
    }
    Ok(report)
}

fn collect_skill_paths(
    input: &Path,
    skills: &StringOrArray,
    dirs: &mut Vec<PathBuf>,
    direct_roots: &mut Vec<PathBuf>,
) {
    let mut visit = |raw: &str| {
        let cleaned = raw.trim_start_matches("./").trim_start_matches('/');
        if cleaned.is_empty() {
            return;
        }
        let resolved = input.join(cleaned);
        if !resolved.exists() {
            return;
        }
        if resolved.join("SKILL.md").is_file() {
            direct_roots.push(resolved);
        } else {
            dirs.push(resolved);
        }
    };
    match skills {
        StringOrArray::Single(p) => visit(p),
        StringOrArray::Multiple(paths) => {
            for p in paths {
                visit(p);
            }
        }
    }
}

fn copy_root_docs(input: &Path, output: &Path) -> Result<()> {
    for file in ["LICENSE", "LICENSE.md", "README.md", "CHANGELOG.md"] {
        let src = input.join(file);
        if src.is_file() {
            std::fs::copy(&src, output.join(file))?;
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct ConvertedSkill {
    pub path: PathBuf,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub struct ExtensionEntry {
    pub namespace: String,
    pub source: PathBuf,
    pub dest: PathBuf,
}

impl ExtensionEntry {
    pub fn new(namespace: String, source: PathBuf, dest_rel: &str) -> Self {
        Self {
            namespace,
            source,
            dest: PathBuf::from(dest_rel),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_manifest_less_plugin() {
        let temp = tempfile::tempdir().unwrap();
        let plugin = temp.path().join("my-tool");
        std::fs::create_dir_all(plugin.join("skills").join("demo")).unwrap();
        std::fs::write(
            plugin.join("skills").join("demo").join("SKILL.md"),
            "---\nname: demo\ndescription: Demo skill\n---\n\nBody.",
        )
        .unwrap();

        let output = temp.path().join("out");
        let report = convert_single(&plugin, &output, &ConvertOptions::default()).unwrap();
        assert!(report.manifest_synthesized);
        assert_eq!(report.skills_converted, 1);
        assert!(output.join("plugin.json").exists());
        assert!(output.join("skills/demo/SKILL.md").exists());
    }

    #[test]
    fn manifest_less_uses_preferred_name() {
        let temp = tempfile::tempdir().unwrap();
        let plugin = temp.path().join("some-tempdir");
        std::fs::create_dir_all(plugin.join("skills").join("demo")).unwrap();
        std::fs::write(
            plugin.join("skills").join("demo").join("SKILL.md"),
            "---\nname: demo\ndescription: Demo\n---\n\nBody.",
        )
        .unwrap();

        let options = ConvertOptions {
            preferred_name: Some("entry-name".to_string()),
            ..ConvertOptions::default()
        };
        let output = temp.path().join("out");
        let report = convert_single(&plugin, &output, &options).unwrap();
        assert!(report.manifest_synthesized);
        assert_eq!(report.name, "entry-name");
        let manifest: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(output.join("plugin.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["name"], "entry-name");
    }

    #[test]
    fn converts_root_skill_plugin() {
        let temp = tempfile::tempdir().unwrap();
        let plugin = temp.path().join("solo");
        std::fs::create_dir_all(&plugin).unwrap();
        std::fs::write(
            plugin.join("SKILL.md"),
            "---\nname: solo-skill\ndescription: Single skill\n---\n\nBody.",
        )
        .unwrap();

        let output = temp.path().join("out");
        let report = convert_single(&plugin, &output, &ConvertOptions::default()).unwrap();
        assert_eq!(report.skills_converted, 1);
        assert!(output.join("skills/solo-skill/SKILL.md").exists());
    }

    #[test]
    fn refuses_non_empty_output_without_force() {
        let temp = tempfile::tempdir().unwrap();
        let plugin = temp.path().join("my-tool");
        std::fs::create_dir_all(&plugin).unwrap();
        std::fs::write(
            plugin.join("SKILL.md"),
            "---\nname: x\ndescription: d\n---\n\nb",
        )
        .unwrap();

        let output = temp.path().join("out");
        std::fs::create_dir_all(&output).unwrap();
        std::fs::write(output.join("stale.txt"), "old").unwrap();

        assert!(convert_single(&plugin, &output, &ConvertOptions::default()).is_err());

        let options = ConvertOptions {
            force: true,
            ..ConvertOptions::default()
        };
        assert!(convert_single(&plugin, &output, &options).is_ok());
    }

    #[test]
    fn convert_directory_detects_collisions() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("plugins");
        for name in ["Foo", "foo"] {
            let plugin = dir.join(name);
            std::fs::create_dir_all(&plugin).unwrap();
            std::fs::write(
                plugin.join("SKILL.md"),
                "---\nname: skill\ndescription: d\n---\n\nb",
            )
            .unwrap();
        }
        let output = temp.path().join("out");
        let result = convert_directory(&dir, &output, &ConvertOptions::default());
        assert!(result.is_err());
        let err = format!("{}", result.err().unwrap());
        assert!(err.contains("Name collision"));
    }

    #[test]
    fn convert_directory_skips_non_plugins() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("plugins");
        let plugin = dir.join("real");
        std::fs::create_dir_all(plugin.join("skills").join("s")).unwrap();
        std::fs::write(
            plugin.join("skills").join("s").join("SKILL.md"),
            "---\nname: s\ndescription: d\n---\n\nb",
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("not-a-plugin")).unwrap();
        std::fs::write(dir.join("not-a-plugin").join("notes.txt"), "x").unwrap();

        let output = temp.path().join("out");
        let report = convert_directory(&dir, &output, &ConvertOptions::default()).unwrap();
        assert_eq!(report.plugins.len(), 1);
        assert_eq!(report.skipped.len(), 1);
    }

    #[test]
    fn copies_root_docs() {
        let temp = tempfile::tempdir().unwrap();
        let plugin = temp.path().join("my-tool");
        std::fs::create_dir_all(&plugin).unwrap();
        std::fs::write(
            plugin.join("SKILL.md"),
            "---\nname: x\ndescription: d\n---\n\nb",
        )
        .unwrap();
        std::fs::write(plugin.join("LICENSE"), "MIT").unwrap();
        std::fs::write(plugin.join("README.md"), "# Readme").unwrap();

        let output = temp.path().join("out");
        convert_single(&plugin, &output, &ConvertOptions::default()).unwrap();
        assert_eq!(
            std::fs::read_to_string(output.join("LICENSE")).unwrap(),
            "MIT"
        );
        assert_eq!(
            std::fs::read_to_string(output.join("README.md")).unwrap(),
            "# Readme"
        );
    }
}
