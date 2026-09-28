use std::collections::HashMap;

use crate::agent_plugins::manifest::{AgentManifest, Author, PLUGIN_SCHEMA};
use crate::claude::manifest::ClaudeManifest;
use crate::validate::name;

pub fn convert(claude: &ClaudeManifest, extension_namespace: &str) -> AgentManifest {
    let mut normalized_name = name::normalize_name(&claude.name);

    if !name::is_valid_plugin_name(&normalized_name) {
        normalized_name = truncated_valid_name(&normalized_name);
    }

    let mut extensions = HashMap::new();
    let mut ext_data = serde_json::Map::new();

    if let Some(ref display_name) = claude.display_name {
        ext_data.insert(
            "displayName".to_string(),
            serde_json::Value::String(display_name.clone()),
        );
    }
    if let Some(ref metadata) = claude.metadata {
        ext_data.insert(
            "metadata".to_string(),
            serde_json::Value::Object(
                metadata
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect(),
            ),
        );
    }
    if let Some(ref skills) = claude.skills {
        ext_data.insert("skills".to_string(), serde_json::to_value(skills).unwrap());
    }
    if let Some(ref commands) = claude.commands {
        ext_data.insert(
            "commands".to_string(),
            serde_json::to_value(commands).unwrap(),
        );
    }
    if let Some(ref agents) = claude.agents {
        ext_data.insert("agents".to_string(), serde_json::to_value(agents).unwrap());
    }
    if let Some(ref hooks) = claude.hooks {
        ext_data.insert("hooks".to_string(), serde_json::to_value(hooks).unwrap());
    }
    if let Some(ref mcp) = claude.mcp_servers {
        ext_data.insert("mcpServers".to_string(), serde_json::to_value(mcp).unwrap());
    }
    if let Some(ref output_styles) = claude.output_styles {
        ext_data.insert(
            "outputStyles".to_string(),
            serde_json::to_value(output_styles).unwrap(),
        );
    }
    if let Some(ref lsp) = claude.lsp_servers {
        ext_data.insert("lspServers".to_string(), serde_json::to_value(lsp).unwrap());
    }
    if let Some(ref experimental) = claude.experimental {
        ext_data.insert(
            "experimental".to_string(),
            serde_json::to_value(experimental).unwrap(),
        );
    }
    if let Some(ref deps) = claude.dependencies {
        ext_data.insert(
            "dependencies".to_string(),
            serde_json::to_value(deps).unwrap(),
        );
    }
    if let Some(ref user_config) = claude.user_config {
        ext_data.insert(
            "userConfig".to_string(),
            serde_json::to_value(user_config).unwrap(),
        );
    }
    if let Some(ref channels) = claude.channels {
        ext_data.insert(
            "channels".to_string(),
            serde_json::to_value(channels).unwrap(),
        );
    }
    if let Some(ref workflows) = claude.workflows {
        ext_data.insert(
            "workflows".to_string(),
            serde_json::to_value(workflows).unwrap(),
        );
    }
    if let Some(default_enabled) = claude.default_enabled {
        ext_data.insert(
            "defaultEnabled".to_string(),
            serde_json::Value::Bool(default_enabled),
        );
    }
    if let Some(ref settings) = claude.settings {
        ext_data.insert(
            "settings".to_string(),
            serde_json::to_value(settings).unwrap(),
        );
    }

    if !ext_data.is_empty() {
        extensions.insert(
            extension_namespace.to_string(),
            serde_json::Value::Object(ext_data),
        );
    }

    let extensions = if extensions.is_empty() {
        None
    } else {
        Some(extensions)
    };

    AgentManifest {
        schema: PLUGIN_SCHEMA.to_string(),
        name: normalized_name,
        version: claude.version.clone(),
        description: claude.description.clone(),
        author: claude.author.as_ref().map(|a| Author {
            name: a.name.clone(),
            email: a.email.clone(),
            url: a.url.clone(),
        }),
        homepage: claude.homepage.clone(),
        repository: claude.repository.clone(),
        license: claude.license.clone(),
        keywords: claude.keywords.clone(),
        extensions,
    }
}

fn truncated_valid_name(name: &str) -> String {
    let clean: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '.')
        .collect();
    let clean = collapse_repeated(&clean, '-');
    let clean = collapse_repeated(&clean, '.');
    let clean = clean.trim_matches('-');
    if clean.is_empty() {
        return "unnamed-plugin".to_string();
    }
    let max_len = clean.len().min(64);
    clean[..max_len]
        .trim_end_matches('-')
        .trim_end_matches('.')
        .to_string()
}

fn collapse_repeated(s: &str, ch: char) -> String {
    let ch_str = ch.to_string();
    let double = format!("{}{}", ch_str, ch_str);
    let mut result = s.replace(&double, &ch_str);
    while result.contains(&double) {
        result = result.replace(&double, &ch_str);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_minimal_manifest() {
        let claude = ClaudeManifest {
            name: "my-plugin".to_string(),
            display_name: None,
            version: Some("1.0.0".to_string()),
            description: Some("A test".to_string()),
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
            settings: None,
        };

        let agent = convert(&claude, "com.claude.code");
        assert_eq!(agent.name, "my-plugin");
        assert_eq!(agent.schema, PLUGIN_SCHEMA);
        assert!(agent.extensions.is_none());
    }

    #[test]
    fn convert_with_extensions() {
        let claude = ClaudeManifest {
            name: "my-plugin".to_string(),
            display_name: Some("My Plugin".to_string()),
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
            settings: None,
        };

        let agent = convert(&claude, "com.claude.code");
        let exts = agent.extensions.unwrap();
        let ext = exts.get("com.claude.code").unwrap();
        assert_eq!(ext["displayName"], "My Plugin");
    }

    #[test]
    fn normalize_name() {
        let claude = ClaudeManifest {
            name: "My Plugin!".to_string(),
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
            settings: None,
        };

        let agent = convert(&claude, "com.claude.code");
        assert_eq!(agent.name, "my-plugin");
    }

    #[test]
    fn normalizes_double_hyphen() {
        let claude = ClaudeManifest {
            name: "my--plugin".to_string(),
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
            settings: None,
        };

        let agent = convert(&claude, "com.claude.code");
        assert!(!agent.name.contains("--"));
        assert!(name::is_valid_plugin_name(&agent.name));
    }

    #[test]
    fn handles_too_long_name() {
        let long = "x".repeat(100);
        let claude = ClaudeManifest {
            name: long,
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
            settings: None,
        };

        let agent = convert(&claude, "com.claude.code");
        assert!(agent.name.len() <= 64);
    }
}
