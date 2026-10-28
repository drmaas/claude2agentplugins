use std::collections::{HashMap, HashSet};

use crate::agent_plugins::mcp::{AgentMcpConfig, AgentMcpServer, MCP_SCHEMA};
use crate::agent_plugins::rules;
use crate::claude::mcp::{ClaudeMcpConfig, ClaudeMcpServer};
use crate::convert::env_vars;

const RESERVED_ENV_KEYS: [&str; 2] = ["PLUGIN_ROOT", "PLUGIN_DATA"];

pub fn convert(claude: &ClaudeMcpConfig) -> (AgentMcpConfig, Vec<String>) {
    let mut mcp_servers = HashMap::new();
    let mut all_warnings = Vec::new();

    let mut stdio_count = 0;
    for (name, server) in &claude.mcp_servers {
        if server.is_remote() {
            let (agent_server, warnings) = convert_remote(name, server);
            match agent_server {
                Some(agent_server) => {
                    mcp_servers.insert(name.clone(), agent_server);
                }
                None => {
                    all_warnings.push(format!(
                        "MCP server '{}': transport '{}' has no Agent Plugins equivalent; the original .mcp.json is preserved under the extension namespace",
                        name,
                        server.server_type.as_deref().unwrap_or("remote")
                    ));
                }
            }
            all_warnings.extend(warnings);
            continue;
        }

        match convert_stdio(name, server, &mut all_warnings) {
            Some(agent_server) => {
                mcp_servers.insert(name.clone(), agent_server);
                stdio_count += 1;
            }
            None => {
                all_warnings.push(format!(
                    "MCP server '{}': no command configured; entry skipped (original .mcp.json preserved under the extension namespace)",
                    name
                ));
            }
        }
    }

    if stdio_count > 0 {
        all_warnings.push(
            "MCP stdio servers now default to cwd = plugin root (Claude Code ran them from the project directory); set cwd explicitly if this matters".to_string(),
        );
    }

    (
        AgentMcpConfig {
            schema: MCP_SCHEMA.to_string(),
            mcp_servers,
        },
        all_warnings,
    )
}

fn convert_stdio(
    name: &str,
    server: &ClaudeMcpServer,
    all_warnings: &mut Vec<String>,
) -> Option<AgentMcpServer> {
    let command = server.command.as_deref()?;

    let mut server_warnings = Vec::new();

    let command_result = env_vars::transform_command(command);
    server_warnings.extend(command_result.warnings);
    if !rules::is_valid_stdio_command(&command_result.value)
        && !server_warnings
            .iter()
            .any(|w| w.contains("does not expand placeholders in command"))
    {
        server_warnings.push(format!(
            "command '{}' must be a bare executable name or a './'-relative path (Agent Plugins §7.2.1)",
            command_result.value
        ));
    }

    let args = server.args.as_ref().map(|args| {
        args.iter()
            .map(|arg| {
                let result = env_vars::transform_env_value(arg);
                server_warnings.extend(result.warnings);
                result.value
            })
            .collect()
    });

    let env = server.env.as_ref().map(|env_map| {
        env_map
            .iter()
            .map(|(k, v)| {
                if RESERVED_ENV_KEYS
                    .iter()
                    .any(|r| k.eq_ignore_ascii_case(r))
                {
                    server_warnings.push(format!(
                        "env key '{}' is reserved by Agent Plugins (§9.2) and makes the server entry invalid",
                        k
                    ));
                }
                let value = match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let result = env_vars::transform_env_value(&value);
                server_warnings.extend(result.warnings);
                (k.clone(), result.value)
            })
            .collect()
    });

    let cwd = server.cwd.as_deref().map(|cwd| {
        let result = env_vars::transform_env_value(cwd);
        server_warnings.extend(result.warnings);
        if !rules::is_valid_stdio_cwd(&result.value) {
            server_warnings.push(format!(
                "cwd '{}' is not a valid Agent Plugins form (must be './…', '${{PLUGIN_ROOT}}…', or '${{PLUGIN_DATA}}…'; §7.2.1)",
                result.value
            ));
        }
        result.value
    });

    if server.headers_helper.is_some() {
        server_warnings.push(
            "headersHelper has no Agent Plugins equivalent; static headers were kept, helper output is dropped (raw config preserved in extensions)".to_string(),
        );
    }

    for w in server_warnings {
        all_warnings.push(format!("MCP server '{}': {}", name, w));
    }

    Some(AgentMcpServer::Stdio {
        command: command_result.value,
        args,
        env,
        cwd,
    })
}

fn convert_remote(name: &str, server: &ClaudeMcpServer) -> (Option<AgentMcpServer>, Vec<String>) {
    let mut warnings = Vec::new();
    if server.headers_helper.is_some() {
        warnings.push(format!(
            "MCP server '{}': headersHelper has no Agent Plugins equivalent; helper output is dropped (raw config preserved in extensions)",
            name
        ));
    }

    let Some(url) = server.url.as_deref() else {
        return (None, warnings);
    };
    if let Err(msg) = rules::validate_remote_url(url) {
        warnings.push(format!("MCP server '{}': {}", name, msg));
    }
    if let Some(headers) = &server.headers {
        let mut seen = HashSet::new();
        for (key, value) in headers {
            let lower = key.to_ascii_lowercase();
            if !seen.insert(lower) {
                warnings.push(format!(
                    "MCP server '{}': duplicate header name '{}' (header names are case-insensitive; §7.2.1)",
                    name, key
                ));
            }
            if value.contains("${") {
                warnings.push(format!(
                    "MCP server '{}': header '{}' contains a placeholder; Agent Plugins does not expand placeholders in header values",
                    name, key
                ));
            }
        }
    }
    let headers = server.headers.clone();

    let agent_server = match server.server_type.as_deref() {
        Some("http") => Some(AgentMcpServer::StreamableHttp {
            url: url.to_string(),
            headers,
        }),
        Some("sse") => Some(AgentMcpServer::Sse {
            url: url.to_string(),
            headers,
        }),
        _ => None,
    };
    (agent_server, warnings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::mcp::ClaudeMcpServer;

    fn stdio_server(
        command: &str,
        args: Option<Vec<String>>,
        env: Option<HashMap<String, serde_json::Value>>,
    ) -> ClaudeMcpServer {
        ClaudeMcpServer {
            command: Some(command.to_string()),
            args,
            env,
            server_type: None,
            url: None,
            headers: None,
            headers_helper: None,
            cwd: None,
        }
    }

    #[test]
    fn convert_single_server() {
        let mut servers = HashMap::new();
        servers.insert(
            "test".to_string(),
            stdio_server("node", Some(vec!["server.js".to_string()]), None),
        );
        let claude = ClaudeMcpConfig {
            mcp_servers: servers,
        };
        let (agent, warnings) = convert(&claude);
        assert_eq!(agent.schema, MCP_SCHEMA);
        assert_eq!(agent.mcp_servers.len(), 1);
        assert!(warnings.iter().any(|w| w.contains("cwd")));
    }

    #[test]
    fn adds_type_field() {
        let mut servers = HashMap::new();
        servers.insert("test".to_string(), stdio_server("node", None, None));
        let claude = ClaudeMcpConfig {
            mcp_servers: servers,
        };
        let (agent, _) = convert(&claude);
        let json = serde_json::to_string(&agent).unwrap();
        assert!(json.contains("\"type\":\"stdio\""));
    }

    #[test]
    fn rewrites_command_path() {
        let mut servers = HashMap::new();
        servers.insert(
            "test".to_string(),
            stdio_server("${CLAUDE_PLUGIN_ROOT}/servers/db-server", None, None),
        );
        let claude = ClaudeMcpConfig {
            mcp_servers: servers,
        };
        let (agent, _) = convert(&claude);
        let json = serde_json::to_string(&agent).unwrap();
        assert!(json.contains("\"command\":\"./servers/db-server\""));
        assert!(!json.contains("CLAUDE_PLUGIN_ROOT"));
    }

    #[test]
    fn transforms_args_and_env_vars() {
        let mut servers = HashMap::new();
        let mut env = HashMap::new();
        env.insert(
            "ROOT".to_string(),
            serde_json::Value::String("${CLAUDE_PLUGIN_ROOT}/data".to_string()),
        );
        servers.insert(
            "test".to_string(),
            stdio_server(
                "node",
                Some(vec!["${CLAUDE_PLUGIN_ROOT}/server.js".to_string()]),
                Some(env),
            ),
        );
        let claude = ClaudeMcpConfig {
            mcp_servers: servers,
        };
        let (agent, _) = convert(&claude);
        let json = serde_json::to_string(&agent).unwrap();
        assert!(json.contains("${PLUGIN_ROOT}/server.js"));
        assert!(json.contains("${PLUGIN_ROOT}/data"));
        assert!(!json.contains("CLAUDE_PLUGIN_ROOT"));
    }

    #[test]
    fn warns_on_project_dir() {
        let mut servers = HashMap::new();
        let mut env = HashMap::new();
        env.insert(
            "DIR".to_string(),
            serde_json::Value::String("${CLAUDE_PROJECT_DIR}/test".to_string()),
        );
        servers.insert("test".to_string(), stdio_server("node", None, Some(env)));
        let claude = ClaudeMcpConfig {
            mcp_servers: servers,
        };
        let (_, warnings) = convert(&claude);
        assert!(warnings.iter().any(|w| w.contains("CLAUDE_PROJECT_DIR")));
    }

    #[test]
    fn warns_on_reserved_env_key() {
        let mut servers = HashMap::new();
        let mut env = HashMap::new();
        env.insert(
            "PLUGIN_ROOT".to_string(),
            serde_json::Value::String("/tmp".to_string()),
        );
        servers.insert("test".to_string(), stdio_server("node", None, Some(env)));
        let claude = ClaudeMcpConfig {
            mcp_servers: servers,
        };
        let (_, warnings) = convert(&claude);
        assert!(warnings.iter().any(|w| w.contains("reserved")));
    }

    #[test]
    fn converts_http_server() {
        let mut servers = HashMap::new();
        servers.insert(
            "remote".to_string(),
            ClaudeMcpServer {
                command: None,
                args: None,
                env: None,
                server_type: Some("http".to_string()),
                url: Some("https://example.com/mcp".to_string()),
                headers: None,
                headers_helper: None,
                cwd: None,
            },
        );
        let claude = ClaudeMcpConfig {
            mcp_servers: servers,
        };
        let (agent, _) = convert(&claude);
        assert_eq!(agent.mcp_servers.len(), 1);
        let json = serde_json::to_string(&agent).unwrap();
        assert!(json.contains("\"type\":\"streamable-http\""));
    }

    #[test]
    fn converts_sse_server() {
        let mut servers = HashMap::new();
        servers.insert(
            "legacy".to_string(),
            ClaudeMcpServer {
                command: None,
                args: None,
                env: None,
                server_type: Some("sse".to_string()),
                url: Some("https://example.com/sse".to_string()),
                headers: None,
                headers_helper: None,
                cwd: None,
            },
        );
        let claude = ClaudeMcpConfig {
            mcp_servers: servers,
        };
        let (agent, _) = convert(&claude);
        let json = serde_json::to_string(&agent).unwrap();
        assert!(json.contains("\"type\":\"sse\""));
    }

    #[test]
    fn warns_on_placeholder_in_remote_url() {
        let mut servers = HashMap::new();
        servers.insert(
            "remote".to_string(),
            ClaudeMcpServer {
                command: None,
                args: None,
                env: None,
                server_type: Some("http".to_string()),
                url: Some("${JFROG_URL}/mcp".to_string()),
                headers: Some(HashMap::from([(
                    "Authorization".to_string(),
                    "token ${TOKEN}".to_string(),
                )])),
                headers_helper: None,
                cwd: None,
            },
        );
        let claude = ClaudeMcpConfig {
            mcp_servers: servers,
        };
        let (agent, warnings) = convert(&claude);
        assert_eq!(agent.mcp_servers.len(), 1);
        assert!(warnings.iter().any(|w| w.contains("placeholder")));
    }

    #[test]
    fn warns_on_invalid_cwd_and_absolute_command() {
        let mut servers = HashMap::new();
        servers.insert(
            "bad".to_string(),
            ClaudeMcpServer {
                command: Some("/usr/bin/node".to_string()),
                args: None,
                env: None,
                server_type: None,
                url: None,
                headers: None,
                headers_helper: None,
                cwd: Some("/tmp".to_string()),
            },
        );
        let claude = ClaudeMcpConfig {
            mcp_servers: servers,
        };
        let (_, warnings) = convert(&claude);
        assert!(warnings.iter().any(|w| w.contains("bare executable")));
        assert!(warnings.iter().any(|w| w.contains("cwd")));
    }

    #[test]
    fn warns_on_non_loopback_http() {
        let mut servers = HashMap::new();
        servers.insert(
            "remote".to_string(),
            ClaudeMcpServer {
                command: None,
                args: None,
                env: None,
                server_type: Some("http".to_string()),
                url: Some("http://example.com/mcp".to_string()),
                headers: None,
                headers_helper: None,
                cwd: None,
            },
        );
        let claude = ClaudeMcpConfig {
            mcp_servers: servers,
        };
        let (_, warnings) = convert(&claude);
        assert!(warnings.iter().any(|w| w.contains("HTTPS")));
    }

    #[test]
    fn skips_ws_server_with_warning() {
        let mut servers = HashMap::new();
        servers.insert(
            "ws-srv".to_string(),
            ClaudeMcpServer {
                command: None,
                args: None,
                env: None,
                server_type: Some("ws".to_string()),
                url: Some("ws://example.com/mcp".to_string()),
                headers: None,
                headers_helper: None,
                cwd: None,
            },
        );
        let claude = ClaudeMcpConfig {
            mcp_servers: servers,
        };
        let (agent, warnings) = convert(&claude);
        assert!(agent.mcp_servers.is_empty());
        assert!(warnings.iter().any(|w| w.contains("ws")));
    }
}
