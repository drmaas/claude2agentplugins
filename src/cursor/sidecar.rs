use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::claude::manifest::{ClaudeManifest, StringOrArray, StringOrArrayOrObject};
use crate::error::{Error, Result};

pub struct SidecarFile {
    pub relative: String,
}

pub fn write(
    input: &Path,
    output: &Path,
    namespace: &str,
    manifest: &ClaudeManifest,
    extras: serde_json::Map<String, serde_json::Value>,
    component_extras: &BTreeMap<String, serde_json::Value>,
    hook_sources: &[PathBuf],
) -> Result<Vec<SidecarFile>> {
    if namespace.contains("..") || namespace.contains('/') || namespace.contains('\\') {
        return Err(Error::Conversion(format!(
            "Invalid extension namespace: {namespace}"
        )));
    }
    let root = output.join(namespace);
    let mut written = Vec::new();

    copy_if_file(
        &input.join(".claude-plugin").join("plugin.json"),
        &root.join("plugin.json"),
        "plugin.json",
        &mut written,
    )?;
    copy_if_file(
        &input.join(".mcp.json"),
        &root.join(".mcp.json"),
        ".mcp.json",
        &mut written,
    )?;
    copy_if_file(
        &input.join(".lsp.json"),
        &root.join(".lsp.json"),
        ".lsp.json",
        &mut written,
    )?;
    copy_if_file(
        &input.join("settings.json"),
        &root.join("settings.json"),
        "settings.json",
        &mut written,
    )?;

    for dir in ["output-styles", "themes", "monitors", "workflows"] {
        copy_if_exists(&input.join(dir), &root.join(dir), dir, &mut written)?;
    }

    for source in hook_sources {
        let name = source
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("hooks.json");
        let dest_rel = format!("hooks-original/{name}");
        copy_if_exists(source, &root.join(&dest_rel), &dest_rel, &mut written)?;
    }
    let default_hooks = input.join("hooks").join("hooks.json");
    if default_hooks.is_file() && !hook_sources.iter().any(|p| p == &default_hooks) {
        copy_if_exists(
            &default_hooks,
            &root.join("hooks-original/hooks.json"),
            "hooks-original/hooks.json",
            &mut written,
        )?;
    }

    copy_manifest_paths(input, &root, manifest, &mut written)?;

    if !extras.is_empty() {
        write_json(
            &root.join("manifest-extras.json"),
            &serde_json::Value::Object(extras),
        )?;
        written.push(SidecarFile {
            relative: "manifest-extras.json".to_string(),
        });
    }
    if !component_extras.is_empty() {
        let value = serde_json::Value::Object(component_extras.clone().into_iter().collect());
        write_json(&root.join("component-extras.json"), &value)?;
        written.push(SidecarFile {
            relative: "component-extras.json".to_string(),
        });
    }

    Ok(written)
}

fn copy_manifest_paths(
    input: &Path,
    root: &Path,
    manifest: &ClaudeManifest,
    written: &mut Vec<SidecarFile>,
) -> Result<()> {
    let mut paths = Vec::new();
    if let Some(styles) = &manifest.output_styles {
        collect_strings(styles, &mut paths);
    }
    if let Some(workflows) = &manifest.workflows {
        collect_strings(workflows, &mut paths);
    }
    if let Some(lsp) = &manifest.lsp_servers {
        match lsp {
            StringOrArrayOrObject::Single(p) => paths.push(p.clone()),
            StringOrArrayOrObject::Multiple(list) => paths.extend(list.clone()),
            StringOrArrayOrObject::Object(_) => {}
        }
    }
    if let Some(experimental) = &manifest.experimental {
        if let Some(themes) = &experimental.themes {
            collect_strings(themes, &mut paths);
        }
        if let Some(monitors) = &experimental.monitors {
            collect_strings(monitors, &mut paths);
        }
    }
    for raw in paths {
        let cleaned = raw.trim_start_matches("./").trim_start_matches('/');
        if cleaned.is_empty() || cleaned.contains("..") {
            continue;
        }
        let source = input.join(cleaned);
        if source.exists() {
            copy_if_exists(&source, &root.join(cleaned), cleaned, written)?;
        }
    }
    Ok(())
}

fn collect_strings(value: &StringOrArray, out: &mut Vec<String>) {
    match value {
        StringOrArray::Single(p) => out.push(p.clone()),
        StringOrArray::Multiple(list) => out.extend(list.clone()),
    }
}

fn copy_if_file(
    source: &Path,
    dest: &Path,
    relative: &str,
    written: &mut Vec<SidecarFile>,
) -> Result<()> {
    if source.is_file() {
        copy_if_exists(source, dest, relative, written)?;
    }
    Ok(())
}

fn copy_if_exists(
    source: &Path,
    dest: &Path,
    relative: &str,
    written: &mut Vec<SidecarFile>,
) -> Result<()> {
    if !source.exists() {
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::convert::extensions::copy_entry(source, dest)?;
    written.push(SidecarFile {
        relative: relative.to_string(),
    });
    Ok(())
}

fn write_json(path: &Path, value: &serde_json::Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(value)?)?;
    Ok(())
}
