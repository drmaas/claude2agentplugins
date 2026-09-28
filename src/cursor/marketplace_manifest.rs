use std::path::Path;

use crate::claude::marketplace::Marketplace;
use crate::convert::DirectoryReport;
use crate::error::Result;
use crate::validate::name;

pub fn write_from_reports(output: &Path, report: &DirectoryReport) -> Result<()> {
    let name = output
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("converted");
    write_index(
        output,
        &name::normalize_name(name),
        "c2ap",
        None,
        Some("Converted from a directory of Claude plugins"),
        &plugin_entries(report),
    )
}

pub fn write_from_claude(
    output: &Path,
    marketplace: &Marketplace,
    report: &DirectoryReport,
) -> Result<()> {
    let name = marketplace
        .name
        .as_deref()
        .map(name::normalize_name)
        .filter(|n| name::is_valid_plugin_name(n))
        .unwrap_or_else(|| "converted".to_string());
    let (owner_name, owner_email) = owner_fields(marketplace);
    let description = None;
    write_index(
        output,
        &name,
        &owner_name,
        owner_email.as_deref(),
        description,
        &plugin_entries(report),
    )
}

fn plugin_entries(report: &DirectoryReport) -> Vec<serde_json::Value> {
    report
        .plugins
        .iter()
        .map(|plugin| {
            let mut entry = serde_json::Map::new();
            entry.insert(
                "name".to_string(),
                serde_json::Value::String(plugin.name.clone()),
            );
            let source = plugin
                .output
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(plugin.name.as_str())
                .to_string();
            entry.insert("source".to_string(), serde_json::Value::String(source));
            if let Some(description) = read_description(&plugin.output) {
                entry.insert(
                    "description".to_string(),
                    serde_json::Value::String(description),
                );
            }
            serde_json::Value::Object(entry)
        })
        .collect()
}

fn read_description(plugin_dir: &Path) -> Option<String> {
    let path = plugin_dir.join(".cursor-plugin").join("plugin.json");
    let content = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&content).ok()?;
    value
        .get("description")
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

fn owner_fields(marketplace: &Marketplace) -> (String, Option<String>) {
    let Some(owner) = &marketplace.owner else {
        return ("c2ap".to_string(), None);
    };
    match owner {
        serde_json::Value::String(name) if !name.is_empty() => (name.clone(), None),
        serde_json::Value::Object(map) => {
            let name = map
                .get("name")
                .and_then(|v| v.as_str())
                .filter(|n| !n.is_empty())
                .unwrap_or("c2ap")
                .to_string();
            let email = map
                .get("email")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            (name, email)
        }
        _ => ("c2ap".to_string(), None),
    }
}

fn write_index(
    output: &Path,
    name: &str,
    owner_name: &str,
    owner_email: Option<&str>,
    description: Option<&str>,
    plugins: &[serde_json::Value],
) -> Result<()> {
    if plugins.is_empty() {
        return Ok(());
    }
    let mut owner = serde_json::Map::new();
    owner.insert(
        "name".to_string(),
        serde_json::Value::String(owner_name.to_string()),
    );
    if let Some(email) = owner_email {
        owner.insert(
            "email".to_string(),
            serde_json::Value::String(email.to_string()),
        );
    }
    let mut manifest = serde_json::Map::new();
    manifest.insert(
        "name".to_string(),
        serde_json::Value::String(name.to_string()),
    );
    manifest.insert("owner".to_string(), serde_json::Value::Object(owner));
    if let Some(description) = description {
        manifest.insert(
            "metadata".to_string(),
            serde_json::json!({ "description": description }),
        );
    }
    manifest.insert(
        "plugins".to_string(),
        serde_json::Value::Array(plugins.to_vec()),
    );
    let dir = output.join(".cursor-plugin");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join("marketplace.json"),
        serde_json::to_string_pretty(&serde_json::Value::Object(manifest))?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::convert::{ConversionReport, Target};

    #[test]
    fn writes_marketplace_from_reports() {
        let temp = tempfile::tempdir().unwrap();
        let plugin = temp.path().join("demo");
        std::fs::create_dir_all(plugin.join(".cursor-plugin")).unwrap();
        std::fs::write(
            plugin.join(".cursor-plugin/plugin.json"),
            r#"{"name":"demo","description":"Demo"}"#,
        )
        .unwrap();
        let report = DirectoryReport {
            plugins: vec![ConversionReport {
                name: "demo".to_string(),
                output: plugin,
                manifest_synthesized: false,
                skills_converted: 1,
                commands_converted: 0,
                mcp_servers: 0,
                extension_dirs: 0,
                warnings: Vec::new(),
                target: Target::Cursor,
            }],
            skipped: Vec::new(),
        };
        let output = temp.path().join("out");
        write_from_reports(&output, &report).unwrap();
        let raw = std::fs::read_to_string(output.join(".cursor-plugin/marketplace.json")).unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["plugins"][0]["name"], "demo");
        assert_eq!(value["plugins"][0]["source"], "demo");
        assert_eq!(value["plugins"][0]["description"], "Demo");
        assert_eq!(value["owner"]["name"], "c2ap");
    }
}
