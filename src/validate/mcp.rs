use crate::claude::mcp::ClaudeMcpConfig;

pub fn validate_mcp_config(config: &ClaudeMcpConfig) -> Vec<String> {
    let mut warnings = Vec::new();

    for (name, server) in &config.mcp_servers {
        if server.command.is_empty() {
            warnings.push(format!("MCP server '{}': empty command", name));
        }
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::mcp::{ClaudeMcpConfig, ClaudeMcpServer};
    use std::collections::HashMap;

    #[test]
    fn no_warnings_for_valid_config() {
        let mut servers = HashMap::new();
        servers.insert(
            "test".to_string(),
            ClaudeMcpServer {
                command: "node".to_string(),
                args: None,
                env: None,
            },
        );
        let config = ClaudeMcpConfig {
            mcp_servers: servers,
        };
        let warnings = validate_mcp_config(&config);
        assert!(warnings.is_empty());
    }

    #[test]
    fn warns_on_empty_command() {
        let mut servers = HashMap::new();
        servers.insert(
            "bad".to_string(),
            ClaudeMcpServer {
                command: String::new(),
                args: None,
                env: None,
            },
        );
        let config = ClaudeMcpConfig {
            mcp_servers: servers,
        };
        let warnings = validate_mcp_config(&config);
        assert!(!warnings.is_empty());
    }
}
