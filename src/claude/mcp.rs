use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::error::{Error, Result};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClaudeMcpConfig {
    #[serde(rename = "mcpServers")]
    pub mcp_servers: HashMap<String, ClaudeMcpServer>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ClaudeMcpServer {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, serde_json::Value>>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub server_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,
    #[serde(rename = "headersHelper", skip_serializing_if = "Option::is_none")]
    pub headers_helper: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
}

impl ClaudeMcpServer {
    pub fn is_remote(&self) -> bool {
        self.server_type.as_deref().is_some_and(|t| t != "stdio")
    }
}

pub fn parse(plugin_dir: &Path) -> Result<ClaudeMcpConfig> {
    let mcp_path = plugin_dir.join(".mcp.json");
    if !mcp_path.exists() {
        return Err(Error::NotFound {
            path: mcp_path.display().to_string(),
            reason: "No .mcp.json found".to_string(),
        });
    }
    let content = std::fs::read_to_string(&mcp_path)?;
    let config: ClaudeMcpConfig = serde_json::from_str(&content)
        .map_err(|e| Error::Conversion(format!("Failed to parse {}: {}", mcp_path.display(), e)))?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_mcp() {
        let json = r#"{"mcpServers": {"test": {"command": "echo"}}}"#;
        let config: ClaudeMcpConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.mcp_servers.len(), 1);
        let server = config.mcp_servers.get("test").unwrap();
        assert_eq!(server.command.as_deref(), Some("echo"));
        assert!(!server.is_remote());
    }

    #[test]
    fn parse_mcp_with_env() {
        let json = r#"{"mcpServers": {"test": {"command": "node", "args": ["server.js"], "env": {"NODE_ENV": "production"}}}}"#;
        let config: ClaudeMcpConfig = serde_json::from_str(json).unwrap();
        let server = config.mcp_servers.get("test").unwrap();
        assert_eq!(server.args.as_ref().unwrap()[0], "server.js");
        assert_eq!(
            server.env.as_ref().unwrap().get("NODE_ENV").unwrap(),
            &serde_json::Value::String("production".to_string())
        );
    }

    #[test]
    fn parse_remote_http_server() {
        let json = r#"{"mcpServers": {"remote": {"type": "http", "url": "https://example.com/mcp", "headers": {"X-Tenant": "acme"}}}}"#;
        let config: ClaudeMcpConfig = serde_json::from_str(json).unwrap();
        let server = config.mcp_servers.get("remote").unwrap();
        assert!(server.is_remote());
        assert_eq!(server.url.as_deref(), Some("https://example.com/mcp"));
        assert!(server.command.is_none());
    }

    #[test]
    fn parse_non_string_env_value() {
        let json =
            r#"{"mcpServers": {"test": {"command": "node", "env": {"RETRIES": 3, "FLAG": true}}}}"#;
        let config: ClaudeMcpConfig = serde_json::from_str(json).unwrap();
        let server = config.mcp_servers.get("test").unwrap();
        assert_eq!(
            server.env.as_ref().unwrap().get("RETRIES").unwrap(),
            &serde_json::Value::Number(serde_json::Number::from(3))
        );
    }
}
