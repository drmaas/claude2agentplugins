use std::collections::HashMap;

use crate::agent_plugins::mcp::{AgentMcpConfig, AgentMcpServer, MCP_SCHEMA};
use crate::claude::mcp::ClaudeMcpConfig;
use crate::convert::env_vars;

pub fn convert(claude: &ClaudeMcpConfig) -> (AgentMcpConfig, Vec<String>) {
    let mut mcp_servers = HashMap::new();
    let mut all_warnings = Vec::new();

    for (name, server) in &claude.mcp_servers {
        let mut server_warnings = Vec::new();
        let env = server.env.as_ref().map(|env_map| {
            env_map
                .iter()
                .map(|(k, v)| {
                    let result = env_vars::transform_env_value(v);
                    server_warnings.extend(result.warnings);
                    (k.clone(), result.value)
                })
                .collect()
        });

        for w in server_warnings {
            all_warnings.push(format!("MCP server '{}': {}", name, w));
        }

        let agent_server = AgentMcpServer::Stdio {
            command: server.command.clone(),
            args: server.args.clone(),
            env,
            cwd: None,
        };

        mcp_servers.insert(name.clone(), agent_server);
    }

    (
        AgentMcpConfig {
            schema: MCP_SCHEMA.to_string(),
            mcp_servers,
        },
        all_warnings,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::mcp::{ClaudeMcpConfig, ClaudeMcpServer};

    #[test]
    fn convert_single_server() {
        let mut servers = HashMap::new();
        servers.insert(
            "test".to_string(),
            ClaudeMcpServer {
                command: "node".to_string(),
                args: Some(vec!["server.js".to_string()]),
                env: None,
            },
        );
        let claude = ClaudeMcpConfig {
            mcp_servers: servers,
        };
        let (agent, warnings) = convert(&claude);
        assert_eq!(agent.schema, MCP_SCHEMA);
        assert_eq!(agent.mcp_servers.len(), 1);
        assert!(warnings.is_empty());
    }

    #[test]
    fn adds_type_field() {
        let mut servers = HashMap::new();
        servers.insert(
            "test".to_string(),
            ClaudeMcpServer {
                command: "node".to_string(),
                args: None,
                env: None,
            },
        );
        let claude = ClaudeMcpConfig {
            mcp_servers: servers,
        };
        let (agent, _) = convert(&claude);
        let json = serde_json::to_string(&agent).unwrap();
        assert!(json.contains("\"type\":\"stdio\""));
    }

    #[test]
    fn transforms_env_vars() {
        let mut servers = HashMap::new();
        let mut env = HashMap::new();
        env.insert("ROOT".to_string(), "${CLAUDE_PLUGIN_ROOT}/data".to_string());
        servers.insert(
            "test".to_string(),
            ClaudeMcpServer {
                command: "node".to_string(),
                args: None,
                env: Some(env),
            },
        );
        let claude = ClaudeMcpConfig {
            mcp_servers: servers,
        };
        let (agent, _) = convert(&claude);
        let json = serde_json::to_string(&agent).unwrap();
        assert!(json.contains("${PLUGIN_ROOT}"));
        assert!(!json.contains("${CLAUDE_PLUGIN_ROOT}"));
    }

    #[test]
    fn warns_on_project_dir() {
        let mut servers = HashMap::new();
        let mut env = HashMap::new();
        env.insert("DIR".to_string(), "${CLAUDE_PROJECT_DIR}/test".to_string());
        servers.insert(
            "test".to_string(),
            ClaudeMcpServer {
                command: "node".to_string(),
                args: None,
                env: Some(env),
            },
        );
        let claude = ClaudeMcpConfig {
            mcp_servers: servers,
        };
        let (_, warnings) = convert(&claude);
        assert!(!warnings.is_empty());
    }
}
