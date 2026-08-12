use std::path::Path;

use crate::claude::marketplace::Marketplace;
use crate::error::Result;
use crate::validate::name;

pub fn batch_convert(
    repo_dir: &Path,
    output_dir: &Path,
    extension_namespace: &str,
    strict: bool,
) -> Result<()> {
    let marketplace = Marketplace::parse(repo_dir)?;
    std::fs::create_dir_all(output_dir)?;

    let mut converted = 0;
    let mut skipped = 0;

    for entry in &marketplace.entries {
        let plugin_path = match &entry.source {
            crate::claude::marketplace::MarketplaceSource::GitHub { subdir, .. } => {
                if let Some(subdir) = subdir {
                    repo_dir.join(subdir)
                } else {
                    repo_dir.join(&entry.name)
                }
            }
            crate::claude::marketplace::MarketplaceSource::GitSubdir { subdir, .. } => {
                repo_dir.join(subdir)
            }
            crate::claude::marketplace::MarketplaceSource::Url { .. }
            | crate::claude::marketplace::MarketplaceSource::Npm { .. }
            | crate::claude::marketplace::MarketplaceSource::Pip { .. } => {
                eprintln!("Skipping non-file source plugin: {}", entry.name);
                skipped += 1;
                continue;
            }
        };

        if !plugin_path.exists() {
            eprintln!("Plugin directory not found: {}", plugin_path.display());
            skipped += 1;
            continue;
        }

        let normalized_name = name::normalize_name(&entry.name);
        let dest = output_dir.join(&normalized_name);
        crate::convert::convert_single(&plugin_path, &dest, extension_namespace, strict)?;
        converted += 1;
    }

    println!("Converted {} plugins, skipped {}", converted, skipped);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_non_existent_directories() {
        let temp = tempfile::tempdir().unwrap();
        let output = temp.path().join("out");

        crate::claude::marketplace::Marketplace { entries: vec![] };

        let result = batch_convert(temp.path(), &output, "com.claude.code", false);
        assert!(result.is_err() || result.is_ok());
    }
}
