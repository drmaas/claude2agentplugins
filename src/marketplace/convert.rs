use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::claude::marketplace::{Marketplace, ResolvedSource};
use crate::convert::{ConvertOptions, DirectoryReport};
use crate::error::{Error, Result};
use crate::marketplace::clone;
use crate::validate::name;

pub fn batch_convert(
    repo_dir: &Path,
    output_dir: &Path,
    options: &ConvertOptions,
) -> Result<DirectoryReport> {
    let marketplace = Marketplace::parse(repo_dir)?;
    let mut report = DirectoryReport::default();
    let mut used_names: HashMap<String, PathBuf> = HashMap::new();

    let plugin_root = marketplace
        .metadata
        .as_ref()
        .and_then(|m| m.plugin_root.clone());

    for entry in &marketplace.plugins {
        let normalized_name = name::normalize_name(&entry.name);
        if let Some(prev) = used_names.get(&normalized_name) {
            report.skipped.push(format!(
                "{}: name collides with {} (both normalize to '{}')",
                entry.name,
                prev.display(),
                normalized_name
            ));
            continue;
        }
        used_names.insert(normalized_name.clone(), PathBuf::from(&normalized_name));
        let dest = output_dir.join(&normalized_name);
        let plugin_options = ConvertOptions {
            preferred_name: Some(entry.name.clone()),
            ..options.clone()
        };
        match resolve_plugin_dir(repo_dir, &entry.source.resolve(), plugin_root.as_deref()) {
            Ok(fetched) => {
                let plugin_path = fetched.path;
                if !plugin_path.exists() {
                    report.skipped.push(format!(
                        "{}: plugin directory not found: {}",
                        entry.name,
                        plugin_path.display()
                    ));
                    continue;
                }
                match crate::convert::convert_single(&plugin_path, &dest, &plugin_options) {
                    Ok(plugin_report) => report.plugins.push(plugin_report),
                    Err(e) => report.skipped.push(format!("{}: {}", entry.name, e)),
                }
            }
            Err(e) => report.skipped.push(format!("{}: {}", entry.name, e)),
        }
    }

    Ok(report)
}

pub(crate) struct FetchedPlugin {
    pub path: PathBuf,
    _keep: Option<tempfile::TempDir>,
}

pub(crate) fn resolve_plugin_dir(
    repo_dir: &Path,
    source: &ResolvedSource,
    plugin_root: Option<&str>,
) -> Result<FetchedPlugin> {
    match source {
        ResolvedSource::Path(p) => {
            let joined = if p.starts_with("./") || p.starts_with('/') {
                p.clone()
            } else if let Some(root) = plugin_root {
                format!("{}/{}", root.trim_end_matches('/'), p)
            } else {
                p.clone()
            };
            Ok(FetchedPlugin {
                path: repo_dir.join(joined.trim_start_matches("./")),
                _keep: None,
            })
        }
        ResolvedSource::Github {
            repo,
            reference,
            sha,
        } => {
            let temp = tempfile::tempdir()?;
            clone::clone_repo(repo, reference.as_deref(), sha.as_deref(), temp.path())?;
            Ok(FetchedPlugin {
                path: temp.path().to_path_buf(),
                _keep: Some(temp),
            })
        }
        ResolvedSource::GitUrl {
            url,
            reference,
            sha,
        } => {
            let temp = tempfile::tempdir()?;
            clone::clone_repo(url, reference.as_deref(), sha.as_deref(), temp.path())?;
            Ok(FetchedPlugin {
                path: temp.path().to_path_buf(),
                _keep: Some(temp),
            })
        }
        ResolvedSource::GitSubdir {
            url,
            path,
            reference,
            sha,
        } => {
            let temp = tempfile::tempdir()?;
            clone::clone_repo(url, reference.as_deref(), sha.as_deref(), temp.path())?;
            Ok(FetchedPlugin {
                path: temp.path().join(path),
                _keep: Some(temp),
            })
        }
        ResolvedSource::Npm { package, version } => {
            let temp = tempfile::tempdir()?;
            fetch_npm_package(package, version.as_deref(), temp.path())?;
            let path = find_plugin_root(&temp.path().join("package"))
                .ok_or_else(|| Error::Conversion("npm package contained no plugin".to_string()))?;
            Ok(FetchedPlugin {
                path,
                _keep: Some(temp),
            })
        }
        ResolvedSource::Pip { package } => Err(Error::Conversion(format!(
            "pip source '{}' is not supported by Claude plugin marketplaces; skipped",
            package
        ))),
        ResolvedSource::Archive { url, .. } => {
            let temp = tempfile::tempdir()?;
            fetch_archive(url, temp.path())?;
            let path = find_plugin_root(temp.path())
                .ok_or_else(|| Error::Conversion("archive contained no plugin".to_string()))?;
            Ok(FetchedPlugin {
                path,
                _keep: Some(temp),
            })
        }
    }
}

fn find_plugin_root(dir: &Path) -> Option<PathBuf> {
    if dir.join(".claude-plugin").join("plugin.json").is_file() {
        return Some(dir.to_path_buf());
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return None;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && path.join(".claude-plugin").join("plugin.json").is_file() {
            return Some(path);
        }
    }
    None
}

fn fetch_npm_package(package: &str, version: Option<&str>, dest: &Path) -> Result<()> {
    let spec = match version {
        Some(v) => format!("{}@{}", package, v),
        None => package.to_string(),
    };
    let output = std::process::Command::new("npm")
        .args(["pack", &spec, "--pack-destination", dest.to_str().unwrap()])
        .output()
        .map_err(|e| Error::Conversion(format!("npm not available: {e}")))?;
    if !output.status.success() {
        return Err(Error::Conversion(format!(
            "npm pack failed for '{}': {}",
            package,
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let tarball_name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let tarball = dest.join(&tarball_name);
    if !tarball.is_file() {
        return Err(Error::Conversion(format!(
            "npm pack produced no tarball for '{}'",
            package
        )));
    }
    let extract_dir = dest.join("package");
    std::fs::create_dir_all(&extract_dir)?;
    let status = std::process::Command::new("tar")
        .args([
            "-xzf",
            tarball.to_str().unwrap(),
            "-C",
            extract_dir.to_str().unwrap(),
        ])
        .status()
        .map_err(|e| Error::Conversion(format!("tar not available: {e}")))?;
    if !status.success() {
        return Err(Error::Conversion(format!(
            "failed to extract npm tarball for '{}'",
            package
        )));
    }
    Ok(())
}

fn fetch_archive(url: &str, dest: &Path) -> Result<()> {
    let zip_path = dest.join("archive.zip");
    let status = std::process::Command::new("curl")
        .args(["-L", "-sS", "-o", zip_path.to_str().unwrap(), url])
        .status()
        .map_err(|e| Error::Conversion(format!("curl not available: {e}")))?;
    if !status.success() {
        return Err(Error::Conversion(format!(
            "failed to download archive: {}",
            url
        )));
    }
    let status = std::process::Command::new("unzip")
        .args([
            "-q",
            "-o",
            zip_path.to_str().unwrap(),
            "-d",
            dest.to_str().unwrap(),
        ])
        .status()
        .map_err(|e| Error::Conversion(format!("unzip not available: {e}")))?;
    if !status.success() {
        return Err(Error::Conversion(format!(
            "failed to extract archive: {}",
            url
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_converts_modern_marketplace() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join(".claude-plugin")).unwrap();
        std::fs::create_dir_all(
            repo.join("plugins")
                .join("alpha")
                .join("skills")
                .join("core"),
        )
        .unwrap();
        std::fs::write(
            repo.join(".claude-plugin").join("marketplace.json"),
            r#"{
                "name": "acme",
                "metadata": {"pluginRoot": "./plugins"},
                "plugins": [
                    {"name": "alpha", "source": "alpha"},
                    {"name": "ghost", "source": "./does-not-exist"}
                ]
            }"#,
        )
        .unwrap();
        std::fs::write(
            repo.join("plugins")
                .join("alpha")
                .join("skills")
                .join("core")
                .join("SKILL.md"),
            "---\nname: core\ndescription: Core\n---\n\nBody.",
        )
        .unwrap();

        let output = temp.path().join("out");
        let report = batch_convert(&repo, &output, &ConvertOptions::default()).unwrap();
        assert_eq!(report.plugins.len(), 1);
        assert_eq!(report.plugins[0].name, "alpha");
        assert!(output.join("alpha/skills/core/SKILL.md").exists());
        assert_eq!(report.skipped.len(), 1);
        assert!(report.skipped[0].contains("ghost"));
    }

    #[test]
    fn finds_plugin_at_root() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join(".claude-plugin")).unwrap();
        std::fs::write(temp.path().join(".claude-plugin").join("plugin.json"), "{}").unwrap();
        assert_eq!(
            find_plugin_root(temp.path()),
            Some(temp.path().to_path_buf())
        );
    }

    #[test]
    fn finds_plugin_in_single_subdir() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("my-plugin").join(".claude-plugin")).unwrap();
        std::fs::write(
            temp.path()
                .join("my-plugin")
                .join(".claude-plugin")
                .join("plugin.json"),
            "{}",
        )
        .unwrap();
        assert_eq!(
            find_plugin_root(temp.path()),
            Some(temp.path().join("my-plugin"))
        );
    }

    #[test]
    fn applies_plugin_root_to_bare_path() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("plugins");
        std::fs::create_dir_all(dir.join("formatter")).unwrap();

        let resolved = resolve_plugin_dir(
            temp.path(),
            &ResolvedSource::Path("formatter".to_string()),
            Some("./plugins"),
        )
        .unwrap();
        assert_eq!(resolved.path, dir.join("formatter"));
    }

    #[test]
    fn resolves_relative_path_source() {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().join("plugins").join("formatter");
        std::fs::create_dir_all(&dir).unwrap();

        let resolved = resolve_plugin_dir(
            temp.path(),
            &ResolvedSource::Path("./plugins/formatter".to_string()),
            None,
        )
        .unwrap();
        assert_eq!(resolved.path, dir);
    }
}
