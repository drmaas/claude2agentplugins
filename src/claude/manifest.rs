use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::error::{Error, Result};

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
#[serde(untagged)]
pub enum StringOrArray {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum StringOrArrayOrObject {
    Single(String),
    Multiple(Vec<String>),
    Object(HashMap<String, serde_json::Value>),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Experimental {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub themes: Option<StringOrArray>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monitors: Option<StringOrArray>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClaudeManifest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
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
    pub metadata: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<StringOrArray>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commands: Option<StringOrArray>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agents: Option<StringOrArray>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hooks: Option<StringOrArrayOrObject>,
    #[serde(rename = "mcpServers")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<StringOrArrayOrObject>,
    #[serde(rename = "outputStyles")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_styles: Option<StringOrArray>,
    #[serde(rename = "lspServers")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lsp_servers: Option<StringOrArrayOrObject>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub experimental: Option<Experimental>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<Vec<serde_json::Value>>,
    #[serde(rename = "userConfig")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_config: Option<HashMap<String, serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channels: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflows: Option<StringOrArray>,
    #[serde(rename = "defaultEnabled")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_enabled: Option<bool>,
}

impl ClaudeManifest {
    pub fn synthesize(name: &str) -> Self {
        ClaudeManifest {
            name: name.to_string(),
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
        }
    }
}

pub fn parse(plugin_dir: &Path) -> Result<ClaudeManifest> {
    let manifest_path = plugin_dir.join(".claude-plugin").join("plugin.json");
    if !manifest_path.exists() {
        return Err(Error::NotFound {
            path: manifest_path.display().to_string(),
            reason: "No .claude-plugin/plugin.json found".to_string(),
        });
    }
    let content = std::fs::read_to_string(&manifest_path)?;
    let manifest: ClaudeManifest = serde_json::from_str(&content).map_err(|e| {
        Error::Conversion(format!(
            "Failed to parse {}: {}",
            manifest_path.display(),
            e
        ))
    })?;
    Ok(manifest)
}
