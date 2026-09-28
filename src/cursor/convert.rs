use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::claude;
use crate::claude::manifest::{ClaudeManifest, StringOrArray};
use crate::claude::skill::Skill;
use crate::convert::{ConversionReport, ConvertOptions, Target};
use crate::cursor::{agents, commands, hooks, manifest, mcp, rules, sidecar, skills};
use crate::error::{Error, Result};

pub fn convert_single(
    input: &Path,
    output: &Path,
    options: &ConvertOptions,
) -> Result<ConversionReport> {
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
        Ok(parsed) => (parsed, false),
        Err(Error::NotFound { .. }) => (ClaudeManifest::synthesize(&plugin_name), true),
        Err(err) => return Err(err),
    };

    let mut warnings = Vec::new();
    if manifest_synthesized {
        warnings.push(format!(
            "No .claude-plugin/plugin.json found; synthesized manifest with name '{plugin_name}'"
        ));
    }

    let built = manifest::build(&claude_plugin);
    warnings.extend(built.warnings);
    warn_unmapped_files(input, &claude_plugin, &mut warnings);

    let claude_skills = load_skills(input, &claude_plugin, &plugin_name)?;
    std::fs::create_dir_all(output)?;

    let written_skills = skills::write_skills(output, &claude_skills)?;
    let mut component_extras: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    stash_extras(
        &mut component_extras,
        "skills",
        written_skills.iter().map(|s| (s.name.as_str(), &s.extras)),
    );
    for skill in &written_skills {
        warnings.extend(skill.warnings.clone());
    }

    let skill_names = written_skills
        .iter()
        .map(|skill| skill.name.clone())
        .collect();
    let written_commands = commands::convert(
        input,
        &claude_plugin,
        output,
        options.convert_commands,
        &skill_names,
    )?;
    stash_extras(
        &mut component_extras,
        "commands",
        written_commands
            .iter()
            .map(|c| (c.name.as_str(), &c.extras)),
    );
    for command in &written_commands {
        warnings.extend(command.warnings.clone());
    }

    let written_agents = agents::convert(input, &claude_plugin, output)?;
    stash_extras(
        &mut component_extras,
        "agents",
        written_agents.iter().map(|a| (a.name.as_str(), &a.extras)),
    );
    for agent in &written_agents {
        warnings.extend(agent.warnings.clone());
    }

    let written_rules = rules::convert(input, output)?;
    if !written_rules.is_empty() {
        warnings.push(
            "Claude plugins have no official rules component; rules/ and CLAUDE.md were mapped to Cursor rules/*.mdc because Cursor plugins load them"
                .to_string(),
        );
    }
    for rule in &written_rules {
        warnings.extend(rule.warnings.clone());
    }

    let settings = crate::claude::settings::load(input, &claude_plugin)?;
    crate::claude::settings::warn_unmapped(&settings, "Cursor plugin", &mut warnings);

    let mut converted_hooks = hooks::convert(input, &claude_plugin)?;
    warnings.append(&mut converted_hooks.warnings);
    let has_bin = input.join("bin").is_dir();
    if has_bin {
        inject_bin_path_hook(&mut converted_hooks.config, &mut warnings);
    }
    if let Some(config) = &converted_hooks.config {
        hooks::write(config, output)?;
    }

    let mcp_servers = match mcp::load(input, &claude_plugin) {
        Ok(Some(config)) => {
            let converted = mcp::convert(&config);
            warnings.extend(converted.warnings);
            mcp::write(&converted.servers, output)?;
            converted.servers.len()
        }
        Ok(None) => 0,
        Err(err) => {
            warnings.push(format!(
                "Failed to parse MCP config: {err} (original file preserved in the sidecar)"
            ));
            0
        }
    };

    manifest::write(&built.manifest, output)?;
    warnings.extend(copy_support_dirs(input, output, has_bin)?);
    if has_bin {
        write_bin_path_script(output)?;
    }
    copy_root_docs(input, output)?;

    let sidecar_files = sidecar::write(
        input,
        output,
        &options.extension_namespace,
        &claude_plugin,
        built.extras,
        &component_extras,
        &converted_hooks.source_files,
    )?;

    let (self_errors, self_warnings) = crate::cursor::validate::validate_plugin(output);
    warnings.extend(self_errors);
    warnings.extend(self_warnings);

    if options.strict && !warnings.is_empty() {
        return Err(Error::Conversion(format!(
            "Conversion produced {} warning(s) and --strict is enabled",
            warnings.len()
        )));
    }

    let commands_converted = written_commands
        .iter()
        .filter(|command| !command.warnings.iter().any(|w| w.contains("skipped")))
        .count();

    Ok(ConversionReport {
        name: built.manifest.name,
        output: output.to_path_buf(),
        manifest_synthesized,
        skills_converted: written_skills.len(),
        commands_converted,
        mcp_servers,
        extension_dirs: sidecar_files.len(),
        warnings,
        target: Target::Cursor,
    })
}

fn stash_extras<'a>(
    dest: &mut BTreeMap<String, serde_json::Value>,
    group: &str,
    items: impl Iterator<Item = (&'a str, &'a BTreeMap<String, serde_json::Value>)>,
) {
    let mut object = serde_json::Map::new();
    for (name, extras) in items {
        if extras.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::to_value(extras) {
            object.insert(name.to_string(), value);
        }
    }
    if !object.is_empty() {
        dest.insert(group.to_string(), serde_json::Value::Object(object));
    }
}

fn warn_unmapped_files(input: &Path, manifest: &ClaudeManifest, warnings: &mut Vec<String>) {
    if manifest.output_styles.is_none() && input.join("output-styles").exists() {
        warnings.push(
            "output-styles/ has no Cursor plugin equivalent; preserved in the sidecar".to_string(),
        );
    }
    if manifest.workflows.is_none() && input.join("workflows").exists() {
        warnings.push(
            "workflows/ has no Cursor plugin equivalent; preserved in the sidecar".to_string(),
        );
    }
    if manifest.lsp_servers.is_none() && input.join(".lsp.json").exists() {
        warnings.push(
            "LSP servers (.lsp.json) have no Cursor plugin equivalent; preserved in the sidecar"
                .to_string(),
        );
    }
    let experimental_themes = manifest
        .experimental
        .as_ref()
        .and_then(|exp| exp.themes.as_ref())
        .is_some();
    let experimental_monitors = manifest
        .experimental
        .as_ref()
        .and_then(|exp| exp.monitors.as_ref())
        .is_some();
    if !experimental_themes && input.join("themes").exists() {
        warnings
            .push("themes/ has no Cursor plugin equivalent; preserved in the sidecar".to_string());
    }
    if !experimental_monitors && input.join("monitors").exists() {
        warnings.push(
            "monitors/ has no Cursor plugin equivalent; preserved in the sidecar".to_string(),
        );
    }
}

fn inject_bin_path_hook(config: &mut Option<serde_json::Value>, warnings: &mut Vec<String>) {
    let entry = serde_json::json!({
        "command": "./scripts/c2ap-prepend-bin-path.py",
        "matcher": "Shell"
    });
    if !matches!(config, Some(serde_json::Value::Object(_))) {
        *config = Some(serde_json::json!({
            "version": 1,
            "hooks": {}
        }));
    }
    let root = config
        .as_mut()
        .and_then(|value| value.as_object_mut())
        .unwrap();
    root.entry("version".to_string())
        .or_insert_with(|| serde_json::json!(1));
    let hooks = root
        .entry("hooks".to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !hooks.is_object() {
        *hooks = serde_json::json!({});
    }
    let hooks = hooks.as_object_mut().unwrap();
    let list = hooks
        .entry("preToolUse".to_string())
        .or_insert_with(|| serde_json::json!([]));
    if let Some(array) = list.as_array_mut() {
        let already = array.iter().any(|item| {
            item.get("command").and_then(|v| v.as_str())
                == Some("./scripts/c2ap-prepend-bin-path.py")
        });
        if !already {
            array.insert(0, entry);
        }
    }
    warnings.push(
        "bin/ PATH approximated with a Cursor preToolUse Shell hook that rewrites commands to prepend the plugin bin/ directory (Cursor has no Bash PATH prepend API)".to_string(),
    );
}

fn write_bin_path_script(output: &Path) -> Result<()> {
    let scripts = output.join("scripts");
    std::fs::create_dir_all(&scripts)?;
    let path = scripts.join("c2ap-prepend-bin-path.py");
    std::fs::write(
        path,
        r#"#!/usr/bin/env python3
import json
import os
import sys

payload = json.load(sys.stdin)
root = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
bin_dir = os.path.join(root, "bin")
command = payload.get("command")
if command is None and isinstance(payload.get("tool_input"), dict):
    command = payload["tool_input"].get("command")
if not isinstance(command, str) or not command.strip():
    print(json.dumps({"permission": "allow"}))
    raise SystemExit(0)
marker = bin_dir + os.pathsep
if command.startswith(f"PATH={bin_dir!r}:") or f"PATH='{bin_dir}:" in command or marker in command:
    print(json.dumps({"permission": "allow"}))
    raise SystemExit(0)
quoted = bin_dir.replace("'", "'\"'\"'")
rewritten = f"PATH='{quoted}:'$PATH {command}"
print(json.dumps({
    "permission": "allow",
    "updated_input": {"command": rewritten},
}))
"#,
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(scripts.join("c2ap-prepend-bin-path.py"))?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(scripts.join("c2ap-prepend-bin-path.py"), perms)?;
    }
    Ok(())
}

fn copy_support_dirs(input: &Path, output: &Path, has_bin: bool) -> Result<Vec<String>> {
    let mut warnings = Vec::new();
    if input.join("scripts").exists() {
        crate::convert::extensions::copy_entry(&input.join("scripts"), &output.join("scripts"))?;
    }
    if has_bin {
        crate::convert::extensions::copy_entry(&input.join("bin"), &output.join("bin"))?;
        warnings.push(
            "bin/ copied to the plugin root so ${CURSOR_PLUGIN_ROOT}/bin commands and the PATH approximation hook keep working"
                .to_string(),
        );
    }
    Ok(warnings)
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

fn load_skills(input: &Path, manifest: &ClaudeManifest, plugin_name: &str) -> Result<Vec<Skill>> {
    let mut extra_skill_dirs = Vec::new();
    let mut direct_skill_roots = Vec::new();
    if let Some(skills) = &manifest.skills {
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
            .unwrap_or(plugin_name)
            .to_string();
        if let Some(skill) = claude::skill::parse_root_skill(&root, &fallback)? {
            claude_skills.push(skill);
        }
    }
    if claude_skills.is_empty()
        && manifest.skills.is_none()
        && let Some(skill) = claude::skill::parse_root_skill(input, plugin_name)?
    {
        claude_skills.push(skill);
    }
    Ok(claude_skills)
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
        StringOrArray::Single(path) => visit(path),
        StringOrArray::Multiple(paths) => {
            for path in paths {
                visit(path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_manifest_less_skill_to_cursor_layout() {
        let temp = tempfile::tempdir().unwrap();
        let plugin = temp.path().join("solo");
        std::fs::create_dir_all(&plugin).unwrap();
        std::fs::write(
            plugin.join("SKILL.md"),
            "---\nname: solo-skill\ndescription: Single skill\n---\n\nBody.\n",
        )
        .unwrap();

        let output = temp.path().join("out");
        let report = convert_single(&plugin, &output, &ConvertOptions::default()).unwrap();
        assert_eq!(report.target, Target::Cursor);
        assert!(report.manifest_synthesized);
        assert_eq!(report.skills_converted, 1);
        assert!(output.join(".cursor-plugin/plugin.json").is_file());
        assert!(!output.join("plugin.json").exists());
        assert!(output.join("skills/solo-skill/SKILL.md").is_file());
        let (errors, validation_warnings) = crate::cursor::validate::validate_plugin(&output);
        assert!(errors.is_empty(), "{errors:?}");
        assert!(validation_warnings.is_empty(), "{validation_warnings:?}");
    }

    #[test]
    fn strict_mode_fails_when_features_are_unmapped() {
        let temp = tempfile::tempdir().unwrap();
        let plugin = temp.path().join("partial");
        std::fs::create_dir_all(&plugin).unwrap();
        std::fs::write(plugin.join("settings.json"), "{}\n").unwrap();
        std::fs::write(
            plugin.join("SKILL.md"),
            "---\nname: partial\ndescription: Partial\n---\n\nBody.\n",
        )
        .unwrap();
        let options = ConvertOptions {
            strict: true,
            ..ConvertOptions::default()
        };
        let err = convert_single(&plugin, &temp.path().join("out"), &options).unwrap_err();
        assert!(err.to_string().contains("--strict"));
    }
}
