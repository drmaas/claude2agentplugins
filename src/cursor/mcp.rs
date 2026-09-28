use std::collections::BTreeMap;
use std::path::Path;

use crate::claude::manifest::{ClaudeManifest, StringOrArrayOrObject};
use crate::claude::mcp::{ClaudeMcpConfig, ClaudeMcpServer};
use crate::cursor::placeholders;
use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub struct CursorMcpServer {
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: Option<BTreeMap<String, String>>,
    pub cwd: Option<String>,
    pub url: Option<String>,
    pub headers: Option<BTreeMap<String, String>>,
}

pub struct McpConvert {
    pub servers: BTreeMap<String, CursorMcpServer>,
    pub warnings: Vec<String>,
}

pub fn load(input: &Path, manifest: &ClaudeManifest) -> Result<Option<ClaudeMcpConfig>> {
    if let Some(spec) = &manifest.mcp_servers {
        return load_from_manifest(input, spec);
    }
    let path = input.join(".mcp.json");
    if path.is_file() {
        return crate::claude::mcp::parse(input).map(Some);
    }
    Ok(None)
}

fn load_from_manifest(
    input: &Path,
    spec: &StringOrArrayOrObject,
) -> Result<Option<ClaudeMcpConfig>> {
    match spec {
        StringOrArrayOrObject::Single(raw) => parse_mcp_file(input, raw).map(Some),
        StringOrArrayOrObject::Multiple(paths) => {
            let mut merged = ClaudeMcpConfig {
                mcp_servers: Default::default(),
            };
            for raw in paths {
                let config = parse_mcp_file(input, raw)?;
                for (name, server) in config.mcp_servers {
                    merged.mcp_servers.insert(name, server);
                }
            }
            Ok(Some(merged))
        }
        StringOrArrayOrObject::Object(object) => {
            let value = serde_json::Value::Object(object.clone().into_iter().collect());
            config_from_value(value).map(Some)
        }
    }
}

fn parse_mcp_file(input: &Path, raw: &str) -> Result<ClaudeMcpConfig> {
    let cleaned = raw.trim_start_matches("./").trim_start_matches('/');
    let path = input.join(cleaned);
    let content = std::fs::read_to_string(&path).map_err(|e| {
        Error::Conversion(format!("Failed to read MCP config {}: {e}", path.display()))
    })?;
    let value: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| Error::Conversion(format!("Failed to parse {}: {e}", path.display())))?;
    config_from_value(value)
}

fn config_from_value(value: serde_json::Value) -> Result<ClaudeMcpConfig> {
    if value.get("mcpServers").is_some() {
        return serde_json::from_value(value)
            .map_err(|e| Error::Conversion(format!("Failed to parse MCP config: {e}")));
    }
    let wrapped = serde_json::json!({ "mcpServers": value });
    serde_json::from_value(wrapped)
        .map_err(|e| Error::Conversion(format!("Failed to parse inline MCP servers: {e}")))
}

pub fn convert(config: &ClaudeMcpConfig) -> McpConvert {
    let mut servers = BTreeMap::new();
    let mut warnings = Vec::new();

    let mut names: Vec<_> = config.mcp_servers.keys().cloned().collect();
    names.sort();
    for name in names {
        let server = &config.mcp_servers[&name];
        match convert_server(&name, server) {
            ServerConvert::Mapped(mapped, server_warnings) => {
                warnings.extend(server_warnings);
                servers.insert(name, mapped);
            }
            ServerConvert::Skipped(reason) => warnings.push(reason),
        }
    }

    McpConvert { servers, warnings }
}

enum ServerConvert {
    Mapped(CursorMcpServer, Vec<String>),
    Skipped(String),
}

fn convert_server(name: &str, server: &ClaudeMcpServer) -> ServerConvert {
    let mut warnings = Vec::new();
    if server.headers_helper.is_some() {
        warnings.push(format!(
            "MCP server '{name}': headersHelper has no Cursor equivalent; helper output is dropped (original .mcp.json preserved in the sidecar)"
        ));
    }

    let transport = server.server_type.as_deref();
    if matches!(transport, Some("ws" | "websocket")) {
        return ServerConvert::Skipped(format!(
            "MCP server '{name}': transport '{}' has no Cursor plugin equivalent; entry skipped (original config preserved in the sidecar)",
            transport.unwrap_or("ws")
        ));
    }

    if server.is_remote() || (server.command.is_none() && server.url.is_some()) {
        let Some(url) = server.url.clone() else {
            return ServerConvert::Skipped(format!(
                "MCP server '{name}': remote server has no url; entry skipped"
            ));
        };
        let rewritten =
            placeholders::rewrite_placeholders(&url, &format!("MCP server '{name}' url"));
        warnings.extend(rewritten.warnings);
        if transport == Some("sse") {
            warnings.push(format!(
                "MCP server '{name}': sse transport written as a url entry; Cursor plugin mcp.json infers transport from the url"
            ));
        }
        let headers = server.headers.as_ref().map(|headers| {
            headers
                .iter()
                .map(|(key, value)| {
                    let rewritten = placeholders::rewrite_placeholders(
                        value,
                        &format!("MCP server '{name}' header '{key}'"),
                    );
                    warnings.extend(rewritten.warnings);
                    (key.clone(), rewritten.value)
                })
                .collect()
        });
        return ServerConvert::Mapped(
            CursorMcpServer {
                command: None,
                args: None,
                env: None,
                cwd: None,
                url: Some(rewritten.value),
                headers,
            },
            warnings,
        );
    }

    let Some(command) = server.command.as_deref() else {
        return ServerConvert::Skipped(format!(
            "MCP server '{name}': no command configured; entry skipped (original config preserved in the sidecar)"
        ));
    };
    let command =
        placeholders::rewrite_placeholders(command, &format!("MCP server '{name}' command"));
    warnings.extend(command.warnings);

    let args = server.args.as_ref().map(|args| {
        args.iter()
            .map(|arg| {
                let rewritten =
                    placeholders::rewrite_placeholders(arg, &format!("MCP server '{name}' arg"));
                warnings.extend(rewritten.warnings);
                rewritten.value
            })
            .collect()
    });
    let env = server.env.as_ref().map(|env| {
        let mut mapped = BTreeMap::new();
        for (key, value) in env {
            let raw = match value {
                serde_json::Value::String(s) => s.clone(),
                other => {
                    warnings.push(format!(
                        "MCP server '{name}': env '{key}' is not a string; converted to '{other}'"
                    ));
                    other.to_string()
                }
            };
            let rewritten = placeholders::rewrite_placeholders(
                &raw,
                &format!("MCP server '{name}' env '{key}'"),
            );
            warnings.extend(rewritten.warnings);
            mapped.insert(key.clone(), rewritten.value);
        }
        mapped
    });
    let cwd = server.cwd.as_deref().map(|cwd| {
        let rewritten =
            placeholders::rewrite_placeholders(cwd, &format!("MCP server '{name}' cwd"));
        warnings.extend(rewritten.warnings);
        rewritten.value
    });

    ServerConvert::Mapped(
        CursorMcpServer {
            command: Some(command.value),
            args,
            env,
            cwd,
            url: None,
            headers: None,
        },
        warnings,
    )
}

pub fn write(servers: &BTreeMap<String, CursorMcpServer>, output_dir: &Path) -> Result<()> {
    if servers.is_empty() {
        return Ok(());
    }
    let mut mcp_servers = serde_json::Map::new();
    for (name, server) in servers {
        let mut entry = serde_json::Map::new();
        if let Some(command) = &server.command {
            entry.insert(
                "command".to_string(),
                serde_json::Value::String(command.clone()),
            );
        }
        if let Some(args) = &server.args {
            entry.insert("args".to_string(), serde_json::json!(args));
        }
        if let Some(env) = &server.env {
            entry.insert("env".to_string(), serde_json::json!(env));
        }
        if let Some(cwd) = &server.cwd {
            entry.insert("cwd".to_string(), serde_json::Value::String(cwd.clone()));
        }
        if let Some(url) = &server.url {
            entry.insert("url".to_string(), serde_json::Value::String(url.clone()));
        }
        if let Some(headers) = &server.headers {
            entry.insert("headers".to_string(), serde_json::json!(headers));
        }
        mcp_servers.insert(name.clone(), serde_json::Value::Object(entry));
    }
    let value = serde_json::json!({ "mcpServers": mcp_servers });
    std::fs::write(
        output_dir.join("mcp.json"),
        serde_json::to_string_pretty(&value)?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn server(
        command: Option<&str>,
        server_type: Option<&str>,
        url: Option<&str>,
    ) -> ClaudeMcpServer {
        ClaudeMcpServer {
            command: command.map(str::to_string),
            args: None,
            env: None,
            server_type: server_type.map(str::to_string),
            url: url.map(str::to_string),
            headers: None,
            headers_helper: None,
            cwd: None,
        }
    }

    #[test]
    fn rewrites_stdio_root_to_cursor_placeholder() {
        let mut servers = HashMap::new();
        let mut stdio = server(Some("${CLAUDE_PLUGIN_ROOT}/bin/server"), None, None);
        stdio.args = Some(vec!["${CLAUDE_PLUGIN_ROOT}/config.json".to_string()]);
        stdio.env = Some(HashMap::from([(
            "TOKEN".to_string(),
            serde_json::Value::String("${user_config.api_token}".to_string()),
        )]));
        servers.insert("local".to_string(), stdio);
        let converted = convert(&ClaudeMcpConfig {
            mcp_servers: servers,
        });
        let local = converted.servers.get("local").unwrap();
        assert_eq!(
            local.command.as_deref(),
            Some("${CURSOR_PLUGIN_ROOT}/bin/server")
        );
        assert_eq!(
            local.args.as_ref().unwrap()[0],
            "${CURSOR_PLUGIN_ROOT}/config.json"
        );
        assert_eq!(
            local.env.as_ref().unwrap().get("TOKEN").map(String::as_str),
            Some("${api_token}")
        );
    }

    #[test]
    fn skips_ws_and_maps_http() {
        let mut servers = HashMap::new();
        servers.insert(
            "socket".to_string(),
            server(None, Some("ws"), Some("wss://example.com")),
        );
        servers.insert(
            "remote".to_string(),
            server(None, Some("http"), Some("https://example.com/mcp")),
        );
        let converted = convert(&ClaudeMcpConfig {
            mcp_servers: servers,
        });
        assert!(!converted.servers.contains_key("socket"));
        assert!(converted.warnings.iter().any(|w| w.contains("ws")));
        assert_eq!(
            converted.servers.get("remote").unwrap().url.as_deref(),
            Some("https://example.com/mcp")
        );
        assert!(converted.servers.get("remote").unwrap().command.is_none());
    }
}
