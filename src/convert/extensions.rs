use std::path::Path;

use crate::claude::manifest::ClaudeManifest;
use crate::convert::ExtensionEntry;
use crate::error::{Error, Result};

pub fn collect(
    input_dir: &Path,
    _manifest: &ClaudeManifest,
    extension_namespace: &str,
) -> Vec<ExtensionEntry> {
    let mut entries = Vec::new();

    let components = [
        ("commands", "commands"),
        ("agents", "agents"),
        ("hooks", "hooks"),
        ("scripts", "scripts"),
        ("themes", "themes"),
        ("workflows", "workflows"),
        ("monitors", "monitors"),
        ("output-styles", "output-styles"),
    ];

    for (src_dir, dest_dir) in components {
        let src_path = input_dir.join(src_dir);
        if src_path.exists() && src_path.is_dir() {
            entries.push(ExtensionEntry::new(
                extension_namespace.to_string(),
                src_path,
                dest_dir,
            ));
        }
    }

    let lsp_path = input_dir.join(".lsp.json");
    if lsp_path.exists() {
        entries.push(ExtensionEntry::new(
            extension_namespace.to_string(),
            lsp_path,
            "lsp.json",
        ));
    }

    let monitors_json = input_dir.join("monitors").join("monitors.json");
    if monitors_json.exists() {
        entries.push(ExtensionEntry::new(
            extension_namespace.to_string(),
            monitors_json,
            "monitors/monitors.json",
        ));
    }

    entries
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

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dest_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_path)?;
        } else {
            std::fs::copy(entry.path(), &dest_path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collects_existing_components() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("commands")).unwrap();
        std::fs::create_dir_all(temp.path().join("agents")).unwrap();

        let manifest = ClaudeManifest {
            name: "test".to_string(),
            display_name: None,
            version: None,
            description: None,
            author: None,
            homepage: None,
            repository: None,
            license: None,
            keywords: None,
            metadata: None,
            skills: None,
            commands: None,
            agents: None,
            hooks: None,
            mcp_servers: None,
            output_styles: None,
            lsp_servers: None,
            experimental: None,
            dependencies: None,
            user_config: None,
            channels: None,
            workflows: None,
            default_enabled: None,
        };

        let entries = collect(temp.path(), &manifest, "com.claude.code");
        assert!(entries.iter().any(|e| e.dest == Path::new("commands")));
        assert!(entries.iter().any(|e| e.dest == Path::new("agents")));
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
