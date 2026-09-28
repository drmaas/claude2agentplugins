use std::collections::HashMap;
use std::path::Path;

use crate::claude::manifest::{Author, ClaudeManifest};
use crate::error::Result;
use crate::validate::name;

#[derive(Debug, Clone)]
pub struct CursorAuthor {
    pub name: String,
    pub email: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CursorManifest {
    pub name: String,
    pub description: Option<String>,
    pub version: Option<String>,
    pub author: Option<CursorAuthor>,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub license: Option<String>,
    pub keywords: Option<Vec<String>>,
    pub variables: Option<serde_json::Value>,
}

pub struct ManifestBuild {
    pub manifest: CursorManifest,
    pub warnings: Vec<String>,
    pub extras: serde_json::Map<String, serde_json::Value>,
}

pub fn build(claude: &ClaudeManifest) -> ManifestBuild {
    let mut warnings = Vec::new();
    let mut extras = serde_json::Map::new();

    let normalized = ensure_plugin_name(&claude.name);
    if normalized != claude.name {
        warnings.push(format!(
            "Plugin name '{}' normalized to '{}' for Cursor (lowercase kebab-case, 1-64 chars)",
            claude.name, normalized
        ));
    }

    if let Some(display) = &claude.display_name {
        extras.insert(
            "displayName".to_string(),
            serde_json::Value::String(display.clone()),
        );
        warnings
            .push("displayName has no Cursor manifest field; preserved in the sidecar".to_string());
    }

    let author = claude
        .author
        .as_ref()
        .and_then(|author| map_author(author, &mut warnings, &mut extras));

    if let Some(metadata) = &claude.metadata {
        extras.insert(
            "metadata".to_string(),
            serde_json::Value::Object(metadata.clone().into_iter().collect()),
        );
        warnings.push(
            "manifest metadata has no Cursor equivalent; preserved in the sidecar".to_string(),
        );
    }
    if let Some(deps) = &claude.dependencies {
        extras.insert(
            "dependencies".to_string(),
            serde_json::to_value(deps).unwrap_or(serde_json::Value::Null),
        );
        warnings.push(
            "dependencies have no Cursor plugin equivalent; preserved in the sidecar".to_string(),
        );
    }
    if let Some(channels) = &claude.channels {
        extras.insert(
            "channels".to_string(),
            serde_json::to_value(channels).unwrap_or(serde_json::Value::Null),
        );
        warnings.push(
            "channels have no Cursor plugin equivalent; preserved in the sidecar".to_string(),
        );
    }
    if let Some(experimental) = &claude.experimental {
        extras.insert(
            "experimental".to_string(),
            serde_json::to_value(experimental).unwrap_or(serde_json::Value::Null),
        );
        warnings.push(
            "experimental themes/monitors have no Cursor plugin equivalent; preserved in the sidecar"
                .to_string(),
        );
    }
    if let Some(enabled) = claude.default_enabled {
        extras.insert(
            "defaultEnabled".to_string(),
            serde_json::Value::Bool(enabled),
        );
        warnings.push(
            "defaultEnabled has no Cursor manifest field; preserved in the sidecar".to_string(),
        );
    }
    if let Some(styles) = &claude.output_styles {
        extras.insert(
            "outputStyles".to_string(),
            serde_json::to_value(styles).unwrap_or(serde_json::Value::Null),
        );
        warnings.push(
            "output styles have no Cursor plugin equivalent; preserved in the sidecar".to_string(),
        );
    }
    if let Some(lsp) = &claude.lsp_servers {
        extras.insert(
            "lspServers".to_string(),
            serde_json::to_value(lsp).unwrap_or(serde_json::Value::Null),
        );
        warnings.push(
            "LSP servers have no Cursor plugin equivalent; preserved in the sidecar".to_string(),
        );
    }
    if let Some(workflows) = &claude.workflows {
        extras.insert(
            "workflows".to_string(),
            serde_json::to_value(workflows).unwrap_or(serde_json::Value::Null),
        );
        warnings.push(
            "workflows have no Cursor plugin equivalent; preserved in the sidecar".to_string(),
        );
    }

    let variables = claude.user_config.as_ref().and_then(|config| {
        let (schema, variable_warnings) = user_config_to_variables(config);
        warnings.extend(variable_warnings);
        extras.insert(
            "userConfig".to_string(),
            serde_json::Value::Object(config.clone().into_iter().collect()),
        );
        schema
    });

    ManifestBuild {
        manifest: CursorManifest {
            name: normalized,
            description: claude.description.clone(),
            version: claude.version.clone(),
            author,
            homepage: claude.homepage.clone(),
            repository: claude.repository.clone(),
            license: claude.license.clone(),
            keywords: claude.keywords.clone(),
            variables,
        },
        warnings,
        extras,
    }
}

fn map_author(
    author: &Author,
    warnings: &mut Vec<String>,
    extras: &mut serde_json::Map<String, serde_json::Value>,
) -> Option<CursorAuthor> {
    if let Some(url) = &author.url {
        extras.insert(
            "authorUrl".to_string(),
            serde_json::Value::String(url.clone()),
        );
        warnings.push(
            "author.url has no Cursor manifest field (author is name and email); preserved in the sidecar"
                .to_string(),
        );
    }
    let Some(name) = author.name.clone().filter(|n| !n.is_empty()) else {
        if author.email.is_some() {
            warnings.push(
                "Cursor author requires a name; author email preserved in the sidecar".to_string(),
            );
            if let Some(email) = &author.email {
                extras.insert(
                    "authorEmail".to_string(),
                    serde_json::Value::String(email.clone()),
                );
            }
        }
        return None;
    };
    Some(CursorAuthor {
        name,
        email: author.email.clone(),
    })
}

pub fn user_config_to_variables(
    config: &HashMap<String, serde_json::Value>,
) -> (Option<serde_json::Value>, Vec<String>) {
    if config.is_empty() {
        return (None, Vec::new());
    }
    let mut warnings = vec![
        "userConfig mapped to Cursor plugin variables; values are not stored in the plugin — set them in the dashboard (Plugins → Configure)".to_string(),
    ];
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for (key, spec) in config {
        let Some(spec) = spec.as_object() else {
            warnings.push(format!(
                "userConfig.{key} is not an object schema; declared as a string variable"
            ));
            properties.insert(
                key.clone(),
                serde_json::json!({"type": "string", "title": key}),
            );
            continue;
        };

        let raw_type = spec
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("string");
        let json_type = match raw_type {
            "string" | "number" | "boolean" => raw_type,
            "directory" | "file" => {
                warnings.push(format!(
                    "userConfig.{key} type '{raw_type}' has no Cursor variable type; declared as string"
                ));
                "string"
            }
            other => {
                warnings.push(format!(
                    "userConfig.{key} type '{other}' is not a Cursor variable type; declared as string"
                ));
                "string"
            }
        };

        let mut prop = serde_json::Map::new();
        prop.insert(
            "type".to_string(),
            serde_json::Value::String(json_type.to_string()),
        );
        if let Some(title) = spec.get("title").and_then(|v| v.as_str()) {
            prop.insert(
                "title".to_string(),
                serde_json::Value::String(title.to_string()),
            );
        }
        if let Some(description) = spec.get("description").and_then(|v| v.as_str()) {
            prop.insert(
                "description".to_string(),
                serde_json::Value::String(description.to_string()),
            );
        }
        if let Some(default) = spec.get("default")
            && (default.is_string() || default.is_number() || default.is_boolean())
        {
            prop.insert("default".to_string(), default.clone());
        }
        if spec.get("sensitive").and_then(|v| v.as_bool()) == Some(true) {
            warnings.push(format!(
                "userConfig.{key} is sensitive; Cursor variables are dashboard-configured and are not written into the plugin"
            ));
        }
        if spec.get("multiple").and_then(|v| v.as_bool()) == Some(true) {
            warnings.push(format!(
                "userConfig.{key} multiple:true has no direct Cursor variable mapping; declared as a single {json_type}"
            ));
        }
        if let Some(min) = spec.get("min").filter(|v| v.is_number()) {
            prop.insert("minimum".to_string(), min.clone());
        }
        if let Some(max) = spec.get("max").filter(|v| v.is_number()) {
            prop.insert("maximum".to_string(), max.clone());
        }
        if spec.get("required").and_then(|v| v.as_bool()) == Some(true) {
            required.push(key.clone());
        }
        properties.insert(key.clone(), serde_json::Value::Object(prop));
    }

    let mut schema = serde_json::Map::new();
    schema.insert(
        "type".to_string(),
        serde_json::Value::String("object".to_string()),
    );
    schema.insert(
        "properties".to_string(),
        serde_json::Value::Object(properties),
    );
    if !required.is_empty() {
        required.sort();
        schema.insert("required".to_string(), serde_json::json!(required));
    }
    (Some(serde_json::Value::Object(schema)), warnings)
}

pub fn write(manifest: &CursorManifest, output_dir: &Path) -> Result<()> {
    let mut value = serde_json::Map::new();
    value.insert(
        "name".to_string(),
        serde_json::Value::String(manifest.name.clone()),
    );
    if let Some(description) = &manifest.description {
        value.insert(
            "description".to_string(),
            serde_json::Value::String(description.clone()),
        );
    }
    if let Some(version) = &manifest.version {
        value.insert(
            "version".to_string(),
            serde_json::Value::String(version.clone()),
        );
    }
    if let Some(author) = &manifest.author {
        let mut author_value = serde_json::Map::new();
        author_value.insert(
            "name".to_string(),
            serde_json::Value::String(author.name.clone()),
        );
        if let Some(email) = &author.email {
            author_value.insert(
                "email".to_string(),
                serde_json::Value::String(email.clone()),
            );
        }
        value.insert(
            "author".to_string(),
            serde_json::Value::Object(author_value),
        );
    }
    if let Some(homepage) = &manifest.homepage {
        value.insert(
            "homepage".to_string(),
            serde_json::Value::String(homepage.clone()),
        );
    }
    if let Some(repository) = &manifest.repository {
        value.insert(
            "repository".to_string(),
            serde_json::Value::String(repository.clone()),
        );
    }
    if let Some(license) = &manifest.license {
        value.insert(
            "license".to_string(),
            serde_json::Value::String(license.clone()),
        );
    }
    if let Some(keywords) = &manifest.keywords {
        value.insert("keywords".to_string(), serde_json::json!(keywords));
    }
    if let Some(variables) = &manifest.variables {
        value.insert("variables".to_string(), variables.clone());
    }

    let dir = output_dir.join(".cursor-plugin");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(
        dir.join("plugin.json"),
        serde_json::to_string_pretty(&serde_json::Value::Object(value))?,
    )?;
    Ok(())
}

fn ensure_plugin_name(name: &str) -> String {
    let normalized = name::normalize_name(name);
    if name::is_valid_plugin_name(&normalized) {
        return normalized;
    }
    let clean: String = normalized
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '.')
        .collect();
    let clean = collapse_repeated(&clean, '-');
    let clean = collapse_repeated(&clean, '.');
    let clean = clean.trim_matches(['-', '.']);
    if clean.is_empty() {
        return "unnamed-plugin".to_string();
    }
    let max_len = clean.len().min(64);
    let trimmed = clean[..max_len].trim_end_matches(['-', '.']);
    if name::is_valid_plugin_name(trimmed) {
        trimmed.to_string()
    } else if trimmed.is_empty() {
        "unnamed-plugin".to_string()
    } else {
        trimmed.to_string()
    }
}

fn collapse_repeated(s: &str, ch: char) -> String {
    let double = format!("{ch}{ch}");
    let mut result = s.replace(&double, &ch.to_string());
    while result.contains(&double) {
        result = result.replace(&double, &ch.to_string());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_user_config_to_variables() {
        let mut config = HashMap::new();
        config.insert(
            "api_token".to_string(),
            serde_json::json!({
                "type": "string",
                "title": "API token",
                "description": "Token",
                "sensitive": true,
                "required": true
            }),
        );
        let (schema, warnings) = user_config_to_variables(&config);
        let schema = schema.unwrap();
        assert_eq!(schema["properties"]["api_token"]["type"], "string");
        assert_eq!(schema["required"][0], "api_token");
        assert!(warnings.iter().any(|w| w.contains("sensitive")));
        assert!(warnings.iter().any(|w| w.contains("dashboard")));
    }

    #[test]
    fn normalizes_invalid_name() {
        let claude = ClaudeManifest::synthesize("My Plugin!");
        let built = build(&claude);
        assert_eq!(built.manifest.name, "my-plugin");
    }
}
