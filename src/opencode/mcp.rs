use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use crate::claude::manifest::{ClaudeManifest, StringOrArrayOrObject};
use crate::claude::mcp::{ClaudeMcpConfig, ClaudeMcpServer};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandPart {
    pub text: String,
    pub plugin_relative: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenCodeMcpServer {
    Local {
        command: Vec<CommandPart>,
        cwd_plugin_root: bool,
        cwd: Option<String>,
        environment: Option<BTreeMap<String, String>>,
    },
    Remote {
        url: String,
        headers: Option<BTreeMap<String, String>>,
    },
}

pub struct LoadedMcp {
    pub config: ClaudeMcpConfig,
    pub present: bool,
    pub warnings: Vec<String>,
}

pub struct ConvertedMcp {
    pub servers: BTreeMap<String, OpenCodeMcpServer>,
    pub warnings: Vec<String>,
    pub referenced_roots: Vec<String>,
}

pub fn load(plugin_dir: &Path, manifest: &ClaudeManifest) -> LoadedMcp {
    let mut servers = HashMap::new();
    let mut warnings = Vec::new();
    let mut present = false;

    let mcp_path = plugin_dir.join(".mcp.json");
    if mcp_path.is_file() {
        present = true;
        merge_mcp_file(&mcp_path, &mut servers, &mut warnings);
    }

    if let Some(declared) = &manifest.mcp_servers {
        present = true;
        match declared {
            StringOrArrayOrObject::Object(map) => {
                for (name, value) in map {
                    match serde_json::from_value::<ClaudeMcpServer>(value.clone()) {
                        Ok(server) => {
                            servers.insert(name.clone(), server);
                        }
                        Err(err) => warnings.push(format!(
                            "MCP server '{name}' in plugin.json could not be parsed: {err}"
                        )),
                    }
                }
            }
            StringOrArrayOrObject::Single(path) => {
                ingest_mcp_ref(plugin_dir, path, &mut servers, &mut warnings);
            }
            StringOrArrayOrObject::Multiple(paths) => {
                for path in paths {
                    ingest_mcp_ref(plugin_dir, path, &mut servers, &mut warnings);
                }
            }
        }
    }

    LoadedMcp {
        config: ClaudeMcpConfig {
            mcp_servers: servers,
        },
        present,
        warnings,
    }
}

pub fn convert(config: &ClaudeMcpConfig) -> ConvertedMcp {
    let mut servers = BTreeMap::new();
    let mut warnings = Vec::new();
    let mut roots = BTreeSet::new();

    let mut names: Vec<&String> = config.mcp_servers.keys().collect();
    names.sort();
    for name in names {
        let server = &config.mcp_servers[name];
        if server.is_remote() {
            if let Some(converted) = convert_remote(name, server, &mut warnings) {
                servers.insert(name.clone(), converted);
            }
            continue;
        }
        match convert_local(name, server, &mut warnings, &mut roots) {
            Some(converted) => {
                servers.insert(name.clone(), converted);
            }
            None => warnings.push(format!(
                "MCP server '{name}': no command configured; entry skipped (original .mcp.json preserved in the sidecar)"
            )),
        }
    }

    ConvertedMcp {
        servers,
        warnings,
        referenced_roots: roots.into_iter().collect(),
    }
}

fn convert_local(
    name: &str,
    server: &ClaudeMcpServer,
    warnings: &mut Vec<String>,
    roots: &mut BTreeSet<String>,
) -> Option<OpenCodeMcpServer> {
    let command = server.command.as_deref()?;
    if command.is_empty() {
        warnings.push(format!(
            "MCP server '{name}': command is empty; entry skipped"
        ));
        return None;
    }
    warn_headers_helper(name, server, warnings);

    let context = format!("MCP server '{name}'");
    let mut argv = vec![rewrite_part(command, &context, warnings, roots)];
    if let Some(args) = &server.args {
        for arg in args {
            argv.push(rewrite_part(arg, &context, warnings, roots));
        }
    }

    let environment = server.env.as_ref().map(|env| {
        let mut converted = BTreeMap::new();
        let mut keys: Vec<&String> = env.keys().collect();
        keys.sort();
        for key in keys {
            let raw = match &env[key] {
                serde_json::Value::String(text) => text.clone(),
                other => {
                    warnings.push(format!(
                        "MCP server '{name}': environment '{key}' is not a string; converted to '{other}'"
                    ));
                    other.to_string()
                }
            };
            let part = rewrite_part(&raw, &format!("{context} env '{key}'"), warnings, roots);
            let value = match &part.plugin_relative {
                Some(relative) => format!("./{relative}"),
                None => part.text,
            };
            converted.insert(key.clone(), value);
        }
        converted
    });
    let environment = environment.filter(|env| !env.is_empty());

    let mut cwd_plugin_root = false;
    let cwd = server.cwd.as_deref().map(|cwd| {
        let part = rewrite_part(cwd, &format!("{context} cwd"), warnings, roots);
        if part.plugin_relative.is_some() || part.text == "." {
            cwd_plugin_root = true;
        }
        part.text
    });
    if argv.iter().any(|part| part.plugin_relative.is_some()) {
        cwd_plugin_root = true;
    }

    Some(OpenCodeMcpServer::Local {
        command: argv,
        cwd_plugin_root,
        cwd,
        environment,
    })
}

fn convert_remote(
    name: &str,
    server: &ClaudeMcpServer,
    warnings: &mut Vec<String>,
) -> Option<OpenCodeMcpServer> {
    warn_headers_helper(name, server, warnings);
    match server.server_type.as_deref() {
        Some("ws") => {
            warnings.push(format!(
                "MCP server '{name}': transport 'ws' has no OpenCode v2 equivalent; entry skipped (original .mcp.json preserved in the sidecar)"
            ));
            return None;
        }
        Some("sse") => warnings.push(format!(
            "MCP server '{name}': Claude SSE transport mapped to OpenCode v2 remote MCP; v2 has no separate SSE transport"
        )),
        Some("http") => {}
        other => {
            warnings.push(format!(
                "MCP server '{name}': transport '{}' has no OpenCode v2 equivalent; entry skipped (original .mcp.json preserved in the sidecar)",
                other.unwrap_or("remote")
            ));
            return None;
        }
    }

    let url = server.url.clone().unwrap_or_default();
    if url.is_empty() {
        warnings.push(format!(
            "MCP server '{name}': remote server has no url; entry skipped"
        ));
        return None;
    }
    warn_unexpanded(&url, &format!("MCP server '{name}' url"), warnings);

    let headers = server.headers.as_ref().map(|headers| {
        let mut converted = BTreeMap::new();
        let mut keys: Vec<&String> = headers.keys().collect();
        keys.sort();
        for key in keys {
            let value = &headers[key];
            warn_unexpanded(
                value,
                &format!("MCP server '{name}' header '{key}'"),
                warnings,
            );
            converted.insert(key.clone(), value.clone());
        }
        converted
    });

    Some(OpenCodeMcpServer::Remote {
        url,
        headers: headers.filter(|headers| !headers.is_empty()),
    })
}

fn warn_headers_helper(name: &str, server: &ClaudeMcpServer, warnings: &mut Vec<String>) {
    if server.headers_helper.is_some() {
        warnings.push(format!(
            "MCP server '{name}': headersHelper has no OpenCode v2 equivalent; helper output is dropped (original .mcp.json preserved in the sidecar)"
        ));
    }
}

fn rewrite_part(
    value: &str,
    context: &str,
    warnings: &mut Vec<String>,
    roots: &mut BTreeSet<String>,
) -> CommandPart {
    warn_unexpanded_local(value, context, warnings);
    if !value.contains("${CLAUDE_PLUGIN_ROOT}") {
        record_root(value, roots);
        return CommandPart {
            text: value.to_string(),
            plugin_relative: None,
        };
    }
    warnings.push(format!(
        "{context}: ${{CLAUDE_PLUGIN_ROOT}} rewritten to a path inside the OpenCode v2 plugin package; OpenCode does not expand Claude plugin-root placeholders"
    ));
    let rewritten = value
        .replace("${CLAUDE_PLUGIN_ROOT}/", "./")
        .replace("${CLAUDE_PLUGIN_ROOT}", ".");
    let plugin_relative = relative_plugin_path(&rewritten);
    if let Some(relative) = &plugin_relative {
        record_plugin_root(relative, roots);
    }
    CommandPart {
        text: rewritten,
        plugin_relative,
    }
}

fn relative_plugin_path(value: &str) -> Option<String> {
    let relative = value.strip_prefix("./")?.trim_start_matches('/');
    if relative.is_empty() || relative == "." || relative.contains("..") {
        return None;
    }
    Some(relative.to_string())
}

fn record_plugin_root(relative: &str, roots: &mut BTreeSet<String>) {
    if let Some(top) = relative.split(['/', '\\']).next()
        && !top.is_empty()
        && top != "."
        && top != ".."
    {
        roots.insert(top.to_string());
    }
}

fn record_root(value: &str, roots: &mut BTreeSet<String>) {
    let relative = value.strip_prefix("./").unwrap_or(value);
    if !(value.starts_with("./") || relative.contains('/')) {
        return;
    }
    if let Some(top) = relative.split(['/', '\\']).next()
        && !top.is_empty()
        && top != "."
        && top != ".."
    {
        roots.insert(top.to_string());
    }
}

fn warn_unexpanded_local(value: &str, context: &str, warnings: &mut Vec<String>) {
    if value.contains("${CLAUDE_PLUGIN_DATA}") {
        warnings.push(format!(
            "{context}: ${{CLAUDE_PLUGIN_DATA}} has no OpenCode v2 equivalent"
        ));
    }
    if value.contains("${CLAUDE_PROJECT_DIR}") {
        warnings.push(format!(
            "{context}: ${{CLAUDE_PROJECT_DIR}} has no OpenCode v2 equivalent"
        ));
    }
    if value.contains("${user_config.") {
        warnings.push(format!(
            "{context}: ${{user_config.*}} has no OpenCode v2 equivalent"
        ));
    }
}

fn warn_unexpanded(value: &str, context: &str, warnings: &mut Vec<String>) {
    if value.contains("${CLAUDE_PLUGIN_ROOT}") || value.contains("${CLAUDE_PLUGIN_DATA}") {
        warnings.push(format!(
            "{context}: Claude plugin path placeholder has no OpenCode v2 equivalent"
        ));
    }
    if value.contains("${CLAUDE_PROJECT_DIR}") {
        warnings.push(format!(
            "{context}: ${{CLAUDE_PROJECT_DIR}} has no OpenCode v2 equivalent"
        ));
    }
    if value.contains("${user_config.") {
        warnings.push(format!(
            "{context}: ${{user_config.*}} has no OpenCode v2 equivalent"
        ));
    }
}

fn ingest_mcp_ref(
    plugin_dir: &Path,
    raw: &str,
    servers: &mut HashMap<String, ClaudeMcpServer>,
    warnings: &mut Vec<String>,
) {
    if raw.ends_with(".mcpb") || raw.ends_with(".dxt") || raw.contains("://") {
        warnings.push(format!(
            "MCP reference '{raw}' is a bundle or URL; OpenCode v2 cannot import it (a file inside the plugin is preserved in the sidecar)"
        ));
        return;
    }
    let Some(path) = resolve_inside(plugin_dir, raw) else {
        warnings.push(format!(
            "MCP reference '{raw}' was not found inside the plugin; skipped"
        ));
        return;
    };
    if !path.is_file() {
        warnings.push(format!("MCP reference '{raw}' is not a file; skipped"));
        return;
    }
    merge_mcp_file(&path, servers, warnings);
}

fn merge_mcp_file(
    path: &Path,
    servers: &mut HashMap<String, ClaudeMcpServer>,
    warnings: &mut Vec<String>,
) {
    let content = match std::fs::read_to_string(path) {
        Ok(content) => content,
        Err(err) => {
            warnings.push(format!("Failed to read {}: {err}", path.display()));
            return;
        }
    };
    if let Ok(config) = serde_json::from_str::<ClaudeMcpConfig>(&content) {
        servers.extend(config.mcp_servers);
        return;
    }
    match serde_json::from_str::<HashMap<String, ClaudeMcpServer>>(&content) {
        Ok(map) => servers.extend(map),
        Err(err) => warnings.push(format!("Failed to parse {}: {err}", path.display())),
    }
}

fn resolve_inside(plugin_dir: &Path, raw: &str) -> Option<std::path::PathBuf> {
    if raw.contains("..") || raw.contains("://") {
        return None;
    }
    let cleaned = raw.trim_start_matches("./").trim_start_matches('/');
    if cleaned.is_empty() {
        return None;
    }
    let resolved = plugin_dir.join(cleaned);
    if resolved.starts_with(plugin_dir) && resolved.exists() {
        Some(resolved)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn server(command: &str) -> ClaudeMcpServer {
        ClaudeMcpServer {
            command: Some(command.to_string()),
            args: None,
            env: None,
            server_type: None,
            url: None,
            headers: None,
            headers_helper: None,
            cwd: None,
        }
    }

    #[test]
    fn rewrites_plugin_root_into_package_relative_parts() {
        let mut servers = HashMap::new();
        let mut local = server("${CLAUDE_PLUGIN_ROOT}/servers/index.js");
        local.args = Some(vec!["--flag".to_string()]);
        local.env = Some(HashMap::from([(
            "ROOT".to_string(),
            serde_json::Value::String("${CLAUDE_PLUGIN_ROOT}/data".to_string()),
        )]));
        servers.insert("local".to_string(), local);
        let converted = convert(&ClaudeMcpConfig {
            mcp_servers: servers,
        });
        let OpenCodeMcpServer::Local {
            command,
            environment,
            cwd_plugin_root,
            ..
        } = &converted.servers["local"]
        else {
            panic!("expected local server");
        };
        assert_eq!(
            command[0].plugin_relative.as_deref(),
            Some("servers/index.js")
        );
        assert_eq!(
            environment
                .as_ref()
                .unwrap()
                .get("ROOT")
                .map(String::as_str),
            Some("./data")
        );
        assert!(cwd_plugin_root);
        assert_eq!(
            converted.referenced_roots,
            vec!["data".to_string(), "servers".to_string()]
        );
        assert!(
            converted
                .warnings
                .iter()
                .any(|warning| warning.contains("plugin package"))
        );
    }

    #[test]
    fn maps_http_and_sse_skips_ws() {
        let mut servers = HashMap::new();
        servers.insert(
            "remote".to_string(),
            ClaudeMcpServer {
                command: None,
                args: None,
                env: None,
                server_type: Some("http".to_string()),
                url: Some("https://example.com/mcp".to_string()),
                headers: Some(HashMap::from([(
                    "Authorization".to_string(),
                    "Bearer x".to_string(),
                )])),
                headers_helper: Some(serde_json::json!({"command": "echo"})),
                cwd: None,
            },
        );
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
        servers.insert(
            "socket".to_string(),
            ClaudeMcpServer {
                command: None,
                args: None,
                env: None,
                server_type: Some("ws".to_string()),
                url: Some("wss://example.com/mcp".to_string()),
                headers: None,
                headers_helper: None,
                cwd: None,
            },
        );
        let converted = convert(&ClaudeMcpConfig {
            mcp_servers: servers,
        });
        assert!(matches!(
            converted.servers.get("remote"),
            Some(OpenCodeMcpServer::Remote { .. })
        ));
        assert!(converted.servers.contains_key("legacy"));
        assert!(!converted.servers.contains_key("socket"));
        assert!(
            converted
                .warnings
                .iter()
                .any(|warning| warning.contains("SSE"))
        );
        assert!(
            converted
                .warnings
                .iter()
                .any(|warning| warning.contains("'ws'"))
        );
        assert!(
            converted
                .warnings
                .iter()
                .any(|warning| warning.contains("headersHelper"))
        );
    }

    #[test]
    fn warns_on_unexpanded_placeholders() {
        let mut servers = HashMap::new();
        let mut local = server("node");
        local.env = Some(HashMap::from([(
            "DIR".to_string(),
            serde_json::Value::String("${CLAUDE_PROJECT_DIR}/src".to_string()),
        )]));
        local.args = Some(vec!["${user_config.token}".to_string()]);
        servers.insert("local".to_string(), local);
        let converted = convert(&ClaudeMcpConfig {
            mcp_servers: servers,
        });
        assert!(
            converted
                .warnings
                .iter()
                .any(|warning| warning.contains("CLAUDE_PROJECT_DIR"))
        );
        assert!(
            converted
                .warnings
                .iter()
                .any(|warning| warning.contains("user_config"))
        );
    }
}
