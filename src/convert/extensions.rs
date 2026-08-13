use std::path::{Path, PathBuf};

use crate::claude::manifest::{ClaudeManifest, StringOrArray, StringOrArrayOrObject};
use crate::convert::ExtensionEntry;
use crate::error::{Error, Result};

pub fn collect(
    input_dir: &Path,
    manifest: &ClaudeManifest,
    extension_namespace: &str,
) -> Vec<ExtensionEntry> {
    let mut entries = Vec::new();
    let ns = extension_namespace.to_string();

    let components = [
        "commands",
        "agents",
        "hooks",
        "scripts",
        "themes",
        "workflows",
        "monitors",
        "output-styles",
        "bin",
    ];

    for dir in components {
        let src_path = input_dir.join(dir);
        if src_path.exists() {
            entries.push(ExtensionEntry::new(ns.clone(), src_path, dir));
        }
    }

    for file in [".lsp.json", "settings.json", ".mcp.json"] {
        let src_path = input_dir.join(file);
        if src_path.is_file() {
            entries.push(ExtensionEntry::new(ns.clone(), src_path, file));
        }
    }

    let mut custom_paths: Vec<PathBuf> = Vec::new();
    if let Some(paths) = manifest.commands.as_ref() {
        collect_string_paths(input_dir, paths, &mut custom_paths);
    }
    if let Some(paths) = manifest.agents.as_ref() {
        collect_string_paths(input_dir, paths, &mut custom_paths);
    }
    if let Some(paths) = &manifest.hooks {
        match paths {
            StringOrArrayOrObject::Single(p) => {
                collect_string_paths(
                    input_dir,
                    &StringOrArray::Single(p.clone()),
                    &mut custom_paths,
                );
            }
            StringOrArrayOrObject::Multiple(ps) => {
                collect_string_paths(
                    input_dir,
                    &StringOrArray::Multiple(ps.clone()),
                    &mut custom_paths,
                );
            }
            StringOrArrayOrObject::Object(_) => {}
        }
    }
    if let Some(paths) = manifest.output_styles.as_ref() {
        collect_string_paths(input_dir, paths, &mut custom_paths);
    }
    if let Some(paths) = &manifest.lsp_servers {
        match paths {
            StringOrArrayOrObject::Single(p) => {
                add_resolved_path(input_dir, p, &mut custom_paths);
            }
            StringOrArrayOrObject::Multiple(ps) => {
                for p in ps {
                    add_resolved_path(input_dir, p, &mut custom_paths);
                }
            }
            StringOrArrayOrObject::Object(_) => {}
        }
    }
    if let Some(paths) = manifest.workflows.as_ref() {
        collect_string_paths(input_dir, paths, &mut custom_paths);
    }
    if let Some(exp) = &manifest.experimental {
        if let Some(themes) = &exp.themes {
            collect_string_paths(input_dir, themes, &mut custom_paths);
        }
        if let Some(monitors) = &exp.monitors {
            collect_string_paths(input_dir, monitors, &mut custom_paths);
        }
    }

    let mut seen: Vec<PathBuf> = Vec::new();
    for path in custom_paths {
        if seen.contains(&path) {
            continue;
        }
        seen.push(path.clone());
        let rel = path
            .strip_prefix(input_dir)
            .ok()
            .map(|r| r.to_string_lossy().to_string());
        if let Some(rel) = rel {
            entries.push(ExtensionEntry::new(ns.clone(), path, &rel));
        }
    }

    entries
}

fn collect_string_paths(input_dir: &Path, value: &StringOrArray, out: &mut Vec<PathBuf>) {
    match value {
        StringOrArray::Single(p) => add_resolved_path(input_dir, p, out),
        StringOrArray::Multiple(paths) => {
            for p in paths {
                add_resolved_path(input_dir, p, out);
            }
        }
    }
}

fn add_resolved_path(input_dir: &Path, raw: &str, out: &mut Vec<PathBuf>) {
    let cleaned = raw.trim_start_matches("./").trim_start_matches('/');
    if cleaned.is_empty() {
        return;
    }
    let resolved = input_dir.join(cleaned);
    if resolved.exists() {
        out.push(resolved);
    }
}

pub fn write(entries: &[ExtensionEntry], output_dir: &Path) -> Result<()> {
    for entry in entries {
        if entry.namespace.contains("..")
            || entry.namespace.contains('/')
            || entry.namespace.contains('\\')
        {
            return Err(Error::Conversion(format!(
                "Invalid extension namespace: {}",
                entry.namespace
            )));
        }

        if entry.dest.to_string_lossy().contains("..") || entry.dest.has_root() {
            return Err(Error::Conversion(format!(
                "Invalid extension destination: {}",
                entry.dest.display()
            )));
        }

        let dest = output_dir.join(&entry.namespace).join(&entry.dest);

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        if entry.source.is_dir() {
            copy_dir_recursive(&entry.source, &dest)?;
        } else if entry.source.is_file() {
            std::fs::copy(&entry.source, &dest)?;
        }
    }
    Ok(())
}

pub(crate) fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dest_path = dst.join(entry.file_name());
        copy_entry(&entry.path(), &dest_path)?;
    }
    Ok(())
}

pub(crate) fn copy_entry(src: &Path, dst: &Path) -> Result<()> {
    let file_type = std::fs::symlink_metadata(src)?.file_type();
    if file_type.is_dir() {
        return copy_dir_recursive(src, dst);
    }
    if file_type.is_file() {
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dst)?;
        return Ok(());
    }
    if file_type.is_symlink() {
        let resolved = std::fs::metadata(src);
        if let Ok(meta) = resolved {
            if meta.is_dir() {
                return copy_dir_recursive(src, dst);
            }
            if meta.is_file() {
                if let Some(parent) = dst.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(src, dst)?;
                return Ok(());
            }
        }
        // Broken or special-file symlink: skip silently rather than fail.
        return Ok(());
    }
    // FIFOs, sockets, and other special files are skipped.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_manifest() -> ClaudeManifest {
        ClaudeManifest::synthesize("test")
    }

    #[test]
    fn collects_existing_components() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("commands")).unwrap();
        std::fs::create_dir_all(temp.path().join("agents")).unwrap();
        std::fs::create_dir_all(temp.path().join("bin")).unwrap();

        let entries = collect(temp.path(), &empty_manifest(), "com.claude.code");
        assert!(entries.iter().any(|e| e.dest == Path::new("commands")));
        assert!(entries.iter().any(|e| e.dest == Path::new("agents")));
        assert!(entries.iter().any(|e| e.dest == Path::new("bin")));
    }

    #[test]
    fn collects_custom_paths() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("custom-commands")).unwrap();
        std::fs::create_dir_all(temp.path().join("deep").join("hooks")).unwrap();
        std::fs::write(temp.path().join("extra-hooks.json"), "{}").unwrap();

        let mut manifest = empty_manifest();
        manifest.commands = Some(StringOrArray::Single("./custom-commands".to_string()));
        manifest.hooks = Some(StringOrArrayOrObject::Single(
            "./extra-hooks.json".to_string(),
        ));

        let entries = collect(temp.path(), &manifest, "com.claude.code");
        assert!(
            entries
                .iter()
                .any(|e| e.dest == Path::new("custom-commands"))
        );
        assert!(
            entries
                .iter()
                .any(|e| e.dest == Path::new("extra-hooks.json"))
        );
    }

    #[test]
    fn preserves_settings_and_mcp() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("settings.json"), "{}").unwrap();
        std::fs::write(temp.path().join(".mcp.json"), "{}").unwrap();

        let entries = collect(temp.path(), &empty_manifest(), "com.claude.code");
        assert!(entries.iter().any(|e| e.dest == Path::new("settings.json")));
        assert!(entries.iter().any(|e| e.dest == Path::new(".mcp.json")));
    }

    #[test]
    fn writes_extension_files() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("out");

        std::fs::create_dir_all(temp.path().join("commands")).unwrap();
        std::fs::write(temp.path().join("commands").join("test.md"), "# Hello").unwrap();

        let entries = vec![ExtensionEntry::new(
            "com.claude.code".to_string(),
            temp.path().join("commands"),
            "commands",
        )];

        write(&entries, &output).unwrap();

        let dest = output
            .join("com.claude.code")
            .join("commands")
            .join("test.md");
        assert!(dest.exists());
        assert_eq!(std::fs::read_to_string(dest).unwrap(), "# Hello");
    }

    #[test]
    fn blocks_path_traversal_in_namespace() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("out");
        std::fs::create_dir_all(&output).unwrap();

        let entries = vec![ExtensionEntry::new(
            "../../etc".to_string(),
            temp.path().to_path_buf(),
            "commands",
        )];

        let result = write(&entries, &output);
        assert!(result.is_err());
    }

    #[test]
    fn allows_valid_namespace() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("out");
        std::fs::create_dir_all(output.join("com.claude.code")).unwrap();

        let src_dir = temp.path().join("commands");
        std::fs::create_dir_all(&src_dir).unwrap();
        std::fs::write(src_dir.join("test.md"), "# Hello").unwrap();

        let entries = vec![ExtensionEntry::new(
            "com.claude.code".to_string(),
            src_dir,
            "commands",
        )];

        let result = write(&entries, &output);
        assert!(result.is_ok());
    }
}
