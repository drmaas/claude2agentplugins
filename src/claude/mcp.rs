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
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
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
        assert_eq!(server.command, "echo");
    }

    #[test]
    fn parse_mcp_with_env() {
        let json = r#"{"mcpServers": {"test": {"command": "node", "args": ["server.js"], "env": {"NODE_ENV": "production"}}}}"#;
        let config: ClaudeMcpConfig = serde_json::from_str(json).unwrap();
        let server = config.mcp_servers.get("test").unwrap();
        assert_eq!(server.args.as_ref().unwrap()[0], "server.js");
        assert_eq!(
            server.env.as_ref().unwrap().get("NODE_ENV").unwrap(),
            "production"
        );
    }
}
