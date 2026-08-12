use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::error::Result;

pub const PLUGIN_SCHEMA: &str = "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Author {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentManifest {
    #[serde(rename = "$schema")]
    pub schema: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<Author>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<HashMap<String, serde_json::Value>>,
}

pub fn write(manifest: &AgentManifest, output_dir: &Path) -> Result<()> {
    let path = output_dir.join("plugin.json");
    let json = serde_json::to_string_pretty(manifest)?;
    std::fs::write(&path, json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_minimal_manifest() {
        let manifest = AgentManifest {
            schema: PLUGIN_SCHEMA.to_string(),
            name: "my-plugin".to_string(),
            version: Some("0.1.0".to_string()),
            description: Some("A test plugin".to_string()),
            author: Some(Author {
                name: Some("Tester".to_string()),
                email: None,
                url: None,
            }),
            homepage: None,
            repository: None,
            license: None,
            keywords: None,
            extensions: None,
        };

        let json = serde_json::to_string(&manifest).unwrap();
        assert!(json.contains("\"$schema\""));
        assert!(json.contains("\"my-plugin\""));
    }
}
