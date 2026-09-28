use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::claude::marketplace::Marketplace;
use crate::error::{Error, Result};
use crate::marketplace::clone;
use crate::opencode::{OpenCodeDirectoryReport, OpenCodeOptions};
use crate::validate::name;

pub fn convert_marketplace(
    repo_url: &str,
    branch: &str,
    output: &Path,
    options: &OpenCodeOptions,
) -> Result<OpenCodeDirectoryReport> {
    let local = PathBuf::from(repo_url);
    if local.is_dir() {
        return batch_convert(&local, output, options);
    }

    let temp_dir = tempfile::tempdir()?;
    clone::clone_repo(repo_url, Some(branch), None, temp_dir.path())?;
    batch_convert(temp_dir.path(), output, options)
}

fn batch_convert(
    repo_dir: &Path,
    output_dir: &Path,
    options: &OpenCodeOptions,
) -> Result<OpenCodeDirectoryReport> {
    let marketplace = Marketplace::parse(repo_dir)?;
    let mut report = OpenCodeDirectoryReport::default();
    let mut used_names: HashMap<String, PathBuf> = HashMap::new();
    let plugin_root = marketplace
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.plugin_root.clone());

    for entry in &marketplace.plugins {
        let normalized_name = name::normalize_name(&entry.name);
        if let Some(previous) = used_names.get(&normalized_name) {
            report.skipped.push(format!(
                "{}: name collides with {} (both normalize to '{normalized_name}')",
                entry.name,
                previous.display()
            ));
            continue;
        }
        used_names.insert(normalized_name.clone(), PathBuf::from(&normalized_name));
        let dest = output_dir.join(&normalized_name);
        let plugin_options = OpenCodeOptions {
            preferred_name: Some(entry.name.clone()),
            ..options.clone()
        };
        match crate::marketplace::convert::resolve_plugin_dir(
            repo_dir,
            &entry.source.resolve(),
            plugin_root.as_deref(),
        ) {
            Ok(fetched) => {
                if !fetched.path.exists() {
                    report.skipped.push(format!(
                        "{}: plugin directory not found: {}",
                        entry.name,
                        fetched.path.display()
                    ));
                    continue;
                }
                match crate::opencode::convert_single(&fetched.path, &dest, &plugin_options) {
                    Ok(plugin_report) => report.plugins.push(plugin_report),
                    Err(err) => report.skipped.push(format!("{}: {err}", entry.name)),
                }
            }
            Err(err) => report.skipped.push(format!("{}: {err}", entry.name)),
        }
    }

    if report.plugins.is_empty() && report.skipped.is_empty() {
        return Err(Error::NotFound {
            path: repo_dir.display().to_string(),
            reason: "Marketplace contains no plugins".to_string(),
        });
    }
    Ok(report)
}
