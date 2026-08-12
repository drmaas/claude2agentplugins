pub mod env_vars;
pub mod extensions;
pub mod manifest;
pub mod mcp;
pub mod skills;

use std::path::{Path, PathBuf};

use crate::claude;
use crate::error::{Error, Result};

pub fn convert_single(
    input: &Path,
    output: &Path,
    extension_namespace: &str,
    strict: bool,
) -> Result<()> {
    let claude_plugin = claude::manifest::parse(input)?;
    let claude_skills = claude::skill::parse_all(input)?;
    let claude_mcp = claude::mcp::parse(input).ok();

    std::fs::create_dir_all(output)?;

    let agent_manifest = manifest::convert(&claude_plugin, extension_namespace);
    crate::agent_plugins::manifest::write(&agent_manifest, output)?;

    let mut all_warnings: Vec<String> = Vec::new();

    let converted_skills = skills::convert(input, output, &claude_skills)?;
    for cs in &converted_skills {
        for w in &cs.warnings {
            all_warnings.push(w.clone());
        }
    }

    if let Some(mcp) = &claude_mcp {
        let (agent_mcp, warnings) = mcp::convert(mcp);
        all_warnings.extend(warnings);
        crate::agent_plugins::mcp::write(&agent_mcp, output)?;
    }

    let extensions = extensions::collect(input, &claude_plugin, extension_namespace);
    extensions::write(&extensions, output)?;

    for w in &all_warnings {
        eprintln!("Warning: {}", w);
    }

    if strict && !all_warnings.is_empty() {
        return Err(Error::Conversion(
            "Conversion produced warnings and --strict is enabled (use --help for details)"
                .to_string(),
        ));
    }

    Ok(())
}

pub fn convert_directory(
    input: &Path,
    output: &Path,
    extension_namespace: &str,
    strict: bool,
) -> Result<()> {
    std::fs::create_dir_all(output)?;

    let mut found = false;
    for entry in std::fs::read_dir(input)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let manifest_path = path.join(".claude-plugin").join("plugin.json");
            if manifest_path.exists() {
                found = true;
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown");
                let normalized_name = crate::validate::name::normalize_name(name);
                let plugin_output = output.join(normalized_name);
                convert_single(&path, &plugin_output, extension_namespace, strict)?;
            }
        }
    }

    if !found {
        return Err(crate::error::Error::NotFound {
            path: input.display().to_string(),
            reason: "No plugins found with .claude-plugin/plugin.json".to_string(),
        });
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
