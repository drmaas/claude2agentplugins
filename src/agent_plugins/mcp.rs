use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::error::Result;

pub const MCP_SCHEMA: &str = "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json";

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AgentMcpConfig {
    #[serde(rename = "$schema")]
    pub schema: String,
    #[serde(rename = "mcpServers")]
    pub mcp_servers: HashMap<String, AgentMcpServer>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum AgentMcpServer {
    #[serde(rename = "stdio")]
    Stdio {
        command: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        args: Option<Vec<String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        env: Option<HashMap<String, String>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
    },
    #[serde(rename = "streamable-http")]
    StreamableHttp {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        headers: Option<HashMap<String, String>>,
    },
    #[serde(rename = "sse")]
    Sse {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        headers: Option<HashMap<String, String>>,
    },
}

pub fn write(config: &AgentMcpConfig, output_dir: &Path) -> Result<()> {
    let path = output_dir.join("mcp.json");
    let json = serde_json::to_string_pretty(config)?;
    std::fs::write(&path, json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_stdio_server() {
        let mut servers = HashMap::new();
        servers.insert(
            "my-server".to_string(),
            AgentMcpServer::Stdio {
                command: "node".to_string(),
                args: Some(vec!["server.js".to_string()]),
                env: None,
                cwd: None,
            },
        );

        let config = AgentMcpConfig {
            schema: MCP_SCHEMA.to_string(),
            mcp_servers: servers,
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"type\":\"stdio\""));
        assert!(json.contains("\"command\":\"node\""));
    }

    #[test]
    fn generate_http_server() {
        let mut servers = HashMap::new();
        servers.insert(
            "http-srv".to_string(),
            AgentMcpServer::StreamableHttp {
                url: "https://example.com/mcp".to_string(),
                headers: None,
            },
        );

        let config = AgentMcpConfig {
            schema: MCP_SCHEMA.to_string(),
            mcp_servers: servers,
        };

        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"type\":\"streamable-http\""));
    }
}
