use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::claude::manifest::{ClaudeManifest, StringOrArray, StringOrArrayOrObject};
use crate::error::Result;

pub struct SidecarReport {
    pub entries: usize,
    pub warnings: Vec<String>,
}

pub fn write(
    plugin_root: &Path,
    output: &Path,
    manifest: &ClaudeManifest,
    namespace: &str,
    root_kept: &HashSet<String>,
    hooks_mapped: bool,
) -> Result<SidecarReport> {
    if namespace.contains("..") || namespace.contains('/') || namespace.contains('\\') {
        return Err(crate::error::Error::Conversion(format!(
            "Invalid extension namespace: {namespace}"
        )));
    }

    let dest_root = output.join("extensions").join(namespace);
    let mut copied = HashSet::new();
    let mut warnings = Vec::new();
    let mut warned = HashSet::new();
    let mut entries = 0;

    let hook_reason = if hooks_mapped {
        "Original Claude hooks preserved after mapping command hooks onto ctx.tool.hook / ctx.session.hook / ctx.shell.hook / ctx.permission.hook"
    } else {
        "Claude hooks that could not map onto OpenCode v2 ctx.tool.hook / ctx.session.hook / ctx.shell.hook / ctx.permission.hook"
    };

    let directories = [
        ("hooks", hook_reason),
        ("scripts", "scripts/ has no OpenCode v2 plugin component"),
        ("bin", "bin/ has no OpenCode v2 plugin component"),
        ("workflows", "workflows/ has no OpenCode v2 equivalent"),
        ("monitors", "monitors/ has no OpenCode v2 equivalent"),
        (
            "output-styles",
            "output-styles/ has no OpenCode v2 equivalent",
        ),
        (
            "themes",
            "Claude themes are not OpenCode v2 theme files and were not installed as a plugin",
        ),
        ("evals", "evals/ has no OpenCode v2 equivalent"),
    ];

    for (dir, reason) in directories {
        if root_kept.contains(dir) {
            continue;
        }
        let source = plugin_root.join(dir);
        if source.exists() {
            copy_rel(
                plugin_root,
                &dest_root,
                Path::new(dir),
                &mut copied,
                &mut entries,
            )?;
            warn_once(
                &mut warned,
                &mut warnings,
                dir,
                format!("{reason}; preserved under extensions/{namespace}/{dir}"),
            );
        }
    }

    for (file, feature, reason) in [
        (
            ".lsp.json",
            "lsp",
            "lspServers have no OpenCode v2 plugin equivalent",
        ),
        (
            "settings.json",
            "settings",
            "settings.json keys other than a mapped agent default remain Claude-specific",
        ),
        (
            ".mcp.json",
            "mcp-original",
            "original .mcp.json preserved because MCP conversion drops fields OpenCode v2 cannot represent",
        ),
    ] {
        let source = plugin_root.join(file);
        if source.is_file() {
            copy_rel(
                plugin_root,
                &dest_root,
                Path::new(file),
                &mut copied,
                &mut entries,
            )?;
            warn_once(
                &mut warned,
                &mut warnings,
                feature,
                format!("{reason}; preserved under extensions/{namespace}/{file}"),
            );
        }
    }

    if plugin_root.join(".claude-plugin").is_dir() {
        copy_rel(
            plugin_root,
            &dest_root,
            Path::new(".claude-plugin"),
            &mut copied,
            &mut entries,
        )?;
    }

    if let Some(StringOrArrayOrObject::Object(map)) = &manifest.hooks {
        let relative = PathBuf::from("hooks").join("inline.json");
        let dest = dest_root.join(&relative);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, serde_json::to_string_pretty(map)?)?;
        copied.insert(relative);
        entries += 1;
        warn_once(
            &mut warned,
            &mut warnings,
            "hooks",
            if hooks_mapped {
                format!(
                    "Original inline Claude hooks preserved under extensions/{namespace}/hooks after mapping onto OpenCode v2 hook APIs"
                )
            } else {
                format!(
                    "Claude hooks that could not map onto OpenCode v2 hook APIs; preserved under extensions/{namespace}/hooks"
                )
            },
        );
    }

    if let Some(StringOrArrayOrObject::Object(map)) = &manifest.lsp_servers {
        let relative = PathBuf::from("lsp").join("inline.json");
        let dest = dest_root.join(&relative);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&dest, serde_json::to_string_pretty(map)?)?;
        entries += 1;
        warn_once(
            &mut warned,
            &mut warnings,
            "lsp",
            format!(
                "lspServers have no OpenCode v2 plugin equivalent; preserved under extensions/{namespace}/lsp/inline.json"
            ),
        );
    }

    copy_declared_paths(
        plugin_root,
        &dest_root,
        manifest,
        &mut CopyState {
            copied: &mut copied,
            entries: &mut entries,
            warned: &mut warned,
            warnings: &mut warnings,
        },
        namespace,
    )?;

    if manifest.user_config.is_some() {
        warn_once(
            &mut warned,
            &mut warnings,
            "userConfig",
            format!(
                "userConfig has no OpenCode v2 equivalent; original manifest preserved under extensions/{namespace}/.claude-plugin"
            ),
        );
    }
    if manifest.dependencies.is_some() {
        warn_once(
            &mut warned,
            &mut warnings,
            "dependencies",
            format!(
                "dependencies has no OpenCode v2 equivalent; original manifest preserved under extensions/{namespace}/.claude-plugin"
            ),
        );
    }
    if manifest.channels.is_some() {
        warn_once(
            &mut warned,
            &mut warnings,
            "channels",
            format!(
                "channels has no OpenCode v2 equivalent; original manifest preserved under extensions/{namespace}/.claude-plugin"
            ),
        );
    }
    if manifest.default_enabled.is_some() {
        warn_once(
            &mut warned,
            &mut warnings,
            "defaultEnabled",
            "defaultEnabled has no OpenCode v2 equivalent; OpenCode plugin enablement is controlled by the plugins list".to_string(),
        );
    }
    if manifest.output_styles.is_some() {
        warn_once(
            &mut warned,
            &mut warnings,
            "output-styles",
            format!(
                "outputStyles has no OpenCode v2 equivalent; preserved under extensions/{namespace}/ when the files exist"
            ),
        );
    }
    if manifest.workflows.is_some() {
        warn_once(
            &mut warned,
            &mut warnings,
            "workflows",
            format!(
                "workflows have no OpenCode v2 equivalent; preserved under extensions/{namespace}/ when the files exist"
            ),
        );
    }
    if manifest.lsp_servers.is_some() {
        warn_once(
            &mut warned,
            &mut warnings,
            "lsp",
            format!(
                "lspServers have no OpenCode v2 plugin equivalent; preserved under extensions/{namespace}/"
            ),
        );
    }
    if manifest.experimental.as_ref().is_some_and(|experimental| {
        experimental.themes.is_some() || experimental.monitors.is_some()
    }) {
        warn_once(
            &mut warned,
            &mut warnings,
            "experimental",
            format!(
                "experimental themes and monitors have no OpenCode v2 equivalent; preserved under extensions/{namespace}/"
            ),
        );
    }

    Ok(SidecarReport { entries, warnings })
}

struct CopyState<'a> {
    copied: &'a mut HashSet<PathBuf>,
    entries: &'a mut usize,
    warned: &'a mut HashSet<String>,
    warnings: &'a mut Vec<String>,
}

fn copy_declared_paths(
    plugin_root: &Path,
    dest_root: &Path,
    manifest: &ClaudeManifest,
    state: &mut CopyState<'_>,
    namespace: &str,
) -> Result<()> {
    let mut paths = Vec::new();
    if let Some(hooks) = &manifest.hooks {
        match hooks {
            StringOrArrayOrObject::Single(path) => paths.push(path.clone()),
            StringOrArrayOrObject::Multiple(items) => paths.extend(items.clone()),
            StringOrArrayOrObject::Object(_) => {}
        }
    }
    push_string_paths(manifest.output_styles.as_ref(), &mut paths);
    push_string_paths(manifest.workflows.as_ref(), &mut paths);
    if let Some(lsp) = &manifest.lsp_servers {
        match lsp {
            StringOrArrayOrObject::Single(path) => paths.push(path.clone()),
            StringOrArrayOrObject::Multiple(items) => paths.extend(items.clone()),
            StringOrArrayOrObject::Object(_) => {}
        }
    }
    if let Some(experimental) = &manifest.experimental {
        push_string_paths(experimental.themes.as_ref(), &mut paths);
        push_string_paths(experimental.monitors.as_ref(), &mut paths);
    }

    for raw in paths {
        if raw.contains("..") || raw.contains("://") {
            state.warnings.push(format!(
                "Component path '{raw}' was not copied into the sidecar"
            ));
            continue;
        }
        let cleaned = raw.trim_start_matches("./").trim_start_matches('/');
        if cleaned.is_empty() {
            continue;
        }
        let relative = PathBuf::from(cleaned);
        if state
            .copied
            .iter()
            .any(|existing| relative.starts_with(existing))
        {
            continue;
        }
        let source = plugin_root.join(&relative);
        if !source.exists() {
            continue;
        }
        copy_rel(
            plugin_root,
            dest_root,
            &relative,
            state.copied,
            state.entries,
        )?;
        warn_once(
            state.warned,
            state.warnings,
            "custom-path",
            format!(
                "Custom Claude component paths have no direct OpenCode v2 component; preserved under extensions/{namespace}/"
            ),
        );
    }
    Ok(())
}

fn push_string_paths(value: Option<&StringOrArray>, out: &mut Vec<String>) {
    match value {
        Some(StringOrArray::Single(path)) => out.push(path.clone()),
        Some(StringOrArray::Multiple(paths)) => out.extend(paths.clone()),
        None => {}
    }
}

fn copy_rel(
    plugin_root: &Path,
    dest_root: &Path,
    relative: &Path,
    copied: &mut HashSet<PathBuf>,
    entries: &mut usize,
) -> Result<()> {
    if !copied.insert(relative.to_path_buf()) {
        return Ok(());
    }
    let source = plugin_root.join(relative);
    if !source.exists() {
        return Ok(());
    }
    crate::convert::extensions::copy_entry(&source, &dest_root.join(relative))?;
    *entries += 1;
    Ok(())
}

fn warn_once(warned: &mut HashSet<String>, warnings: &mut Vec<String>, key: &str, message: String) {
    if warned.insert(key.to_string()) {
        warnings.push(message);
    }
}
