use crate::claude::mcp::ClaudeMcpConfig;

const RESERVED_ENV_KEYS: [&str; 2] = ["PLUGIN_ROOT", "PLUGIN_DATA"];

pub fn validate_mcp_config(config: &ClaudeMcpConfig) -> Vec<String> {
    let mut warnings = Vec::new();

    for (name, server) in &config.mcp_servers {
        if server.command.as_deref().is_some_and(|c| c.is_empty()) {
            warnings.push(format!("MCP server '{}': empty command", name));
        }
        if let Some(env) = &server.env {
            for key in env.keys() {
                if RESERVED_ENV_KEYS
                    .iter()
                    .any(|r| key.eq_ignore_ascii_case(r))
                {
                    warnings.push(format!(
                        "MCP server '{}': env key '{}' is reserved by Agent Plugins",
                        name, key
                    ));
                }
            }
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
                command: Some("node".to_string()),
                args: None,
                env: None,
                server_type: None,
                url: None,
                headers: None,
                headers_helper: None,
                cwd: None,
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
                command: Some(String::new()),
                args: None,
                env: None,
                server_type: None,
                url: None,
                headers: None,
                headers_helper: None,
                cwd: None,
            },
        );
        let config = ClaudeMcpConfig {
            mcp_servers: servers,
        };
        let warnings = validate_mcp_config(&config);
        assert!(!warnings.is_empty());
    }

    #[test]
    fn warns_on_reserved_env_key() {
        let mut servers = HashMap::new();
        let mut env = HashMap::new();
        env.insert(
            "PLUGIN_DATA".to_string(),
            serde_json::Value::String("/tmp".to_string()),
        );
        servers.insert(
            "bad".to_string(),
            ClaudeMcpServer {
                command: Some("node".to_string()),
                args: None,
                env: Some(env),
                server_type: None,
                url: None,
                headers: None,
                headers_helper: None,
                cwd: None,
            },
        );
        let config = ClaudeMcpConfig {
            mcp_servers: servers,
        };
        let warnings = validate_mcp_config(&config);
        assert!(!warnings.is_empty());
    }
}
