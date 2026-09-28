use std::path::Path;

use serde_json::{Map, Value};

use crate::claude::manifest::ClaudeManifest;
use crate::error::Result;
use crate::opencode::agents::RegisteredAgent;
use crate::opencode::commands::RegisteredCommand;
use crate::opencode::mcp::{CommandPart, OpenCodeMcpServer};
use crate::opencode::skills::RegisteredSkill;

pub fn write_package(
    output: &Path,
    manifest: &ClaudeManifest,
    package_name: &str,
    version: &str,
) -> Result<()> {
    let mut package = Map::new();
    package.insert("name".to_string(), Value::String(package_name.to_string()));
    package.insert("version".to_string(), Value::String(version.to_string()));
    package.insert("type".to_string(), Value::String("module".to_string()));
    if let Some(description) = &manifest.description {
        package.insert(
            "description".to_string(),
            Value::String(description.clone()),
        );
    }
    if let Some(license) = &manifest.license {
        package.insert("license".to_string(), Value::String(license.clone()));
    }
    package.insert(
        "exports".to_string(),
        serde_json::json!({ ".": "./src/index.ts" }),
    );
    package.insert(
        "dependencies".to_string(),
        serde_json::json!({ "@opencode/plugin": "latest" }),
    );
    let text = serde_json::to_string_pretty(&Value::Object(package))?;
    std::fs::write(output.join("package.json"), format!("{text}\n"))?;
    Ok(())
}

pub fn write_plugin(
    output: &Path,
    plugin_id: &str,
    skills: &[RegisteredSkill],
    agents: &[RegisteredAgent],
    commands: &[RegisteredCommand],
    servers: &std::collections::BTreeMap<String, OpenCodeMcpServer>,
) -> Result<()> {
    let needs_root = !skills.is_empty()
        || servers.values().any(|server| match server {
            OpenCodeMcpServer::Local {
                command,
                cwd_plugin_root,
                ..
            } => *cwd_plugin_root || command.iter().any(|part| part.plugin_relative.is_some()),
            OpenCodeMcpServer::Remote { .. } => false,
        });

    let mut source = String::new();
    source.push_str("import { Plugin } from \"@opencode/plugin\"\n");
    if needs_root {
        source.push_str("import path from \"node:path\"\n");
        source.push_str("import { fileURLToPath } from \"node:url\"\n\n");
        source.push_str(
            "const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), \"..\")\n\n",
        );
    } else {
        source.push('\n');
    }
    source.push_str("export default Plugin.define({\n");
    source.push_str(&format!("  id: {},\n", js_string(plugin_id)));
    source.push_str("  async setup(ctx) {\n");
    if !skills.is_empty() {
        source.push_str("    await ctx.skill.transform((editor) => {\n");
        for skill in skills {
            source.push_str("      editor.add({\n");
            source.push_str(&format!("        id: {},\n", js_string(&skill.id)));
            source.push_str(&format!("        name: {},\n", js_string(&skill.id)));
            source.push_str(&format!(
                "        description: {},\n",
                js_string(&skill.description)
            ));
            source.push_str(&format!(
                "        location: path.join(root, {}),\n",
                js_string(&format!("{}/SKILL.md", skill.relative_dir))
            ));
            source.push_str(&format!(
                "        content: {},\n",
                js_string(&skill.content)
            ));
            source.push_str("      })\n");
        }
        source.push_str("    })\n");
    }
    if !agents.is_empty() {
        source.push_str("    await ctx.agent.transform((editor) => {\n");
        for agent in agents {
            source.push_str(&format!(
                "      editor.update({}, (agent) => {{\n",
                js_string(&agent.id)
            ));
            source.push_str(&format!(
                "        agent.description = {}\n",
                js_string(&agent.description)
            ));
            source.push_str(&format!("        agent.mode = {}\n", js_string(agent.mode)));
            source.push_str("        agent.hidden = false\n");
            source.push_str(&format!(
                "        agent.system = {}\n",
                js_string(&agent.system)
            ));
            source.push_str(&format!(
                "        agent.permissions = {}\n",
                permissions_literal(&agent.permissions)
            ));
            if let Some(color) = &agent.color {
                source.push_str(&format!("        agent.color = {}\n", js_string(color)));
            }
            if let Some(steps) = agent.steps {
                source.push_str(&format!("        agent.steps = {steps}\n"));
            }
            if let Some((provider, id)) = &agent.model {
                source.push_str(&format!(
                    "        agent.model = {{ providerID: {}, id: {} }}\n",
                    js_string(provider),
                    js_string(id)
                ));
            }
            source.push_str("      })\n");
        }
        source.push_str("    })\n");
    }
    if !commands.is_empty() {
        source.push_str("    await ctx.command.transform((editor) => {\n");
        for command in commands {
            source.push_str("      editor.add({\n");
            source.push_str(&format!("        name: {},\n", js_string(&command.name)));
            source.push_str(&format!(
                "        description: {},\n",
                js_string(&command.description)
            ));
            source.push_str("        execute: async ({ sessionID, prompt, delivery }) => {\n");
            source.push_str("          const args = prompt.text ?? \"\"\n");
            source.push_str(&format!(
                "          const text = {}.replaceAll(\"$ARGUMENTS\", args)\n",
                js_string(&command.template)
            ));
            source.push_str("          await ctx.session.prompt({\n");
            source.push_str("            ...prompt,\n");
            source.push_str("            sessionID,\n");
            source.push_str("            text,\n");
            source.push_str("            delivery,\n");
            source.push_str("          })\n");
            source.push_str("        },\n");
            source.push_str("      })\n");
        }
        source.push_str("    })\n");
    }
    if !servers.is_empty() {
        source.push_str("    await ctx.mcp.transform((editor) => {\n");
        for (name, server) in servers {
            source.push_str(&format!(
                "      editor.set({}, {})\n",
                js_string(name),
                mcp_literal(server)
            ));
        }
        source.push_str("    })\n");
    }
    source.push_str("  },\n");
    source.push_str("})\n");

    let index = output.join("src").join("index.ts");
    if let Some(parent) = index.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(index, source)?;
    Ok(())
}

fn mcp_literal(server: &OpenCodeMcpServer) -> String {
    match server {
        OpenCodeMcpServer::Local {
            command,
            cwd_plugin_root,
            cwd,
            environment,
        } => {
            let mut fields = vec![
                "type: \"local\"".to_string(),
                format!("command: {}", command_literal(command)),
            ];
            if *cwd_plugin_root {
                fields.push("cwd: root".to_string());
            } else if let Some(cwd) = cwd {
                fields.push(format!("cwd: {}", js_string(cwd)));
            }
            if let Some(environment) = environment {
                fields.push(format!("environment: {}", map_literal(environment)));
            }
            format!("{{ {} }}", fields.join(", "))
        }
        OpenCodeMcpServer::Remote { url, headers } => {
            let mut fields = vec![
                "type: \"remote\"".to_string(),
                format!("url: {}", js_string(url)),
            ];
            if let Some(headers) = headers {
                fields.push(format!("headers: {}", map_literal(headers)));
            }
            format!("{{ {} }}", fields.join(", "))
        }
    }
}

fn command_literal(command: &[CommandPart]) -> String {
    let parts: Vec<String> = command
        .iter()
        .map(|part| match &part.plugin_relative {
            Some(relative) => format!("path.join(root, {})", js_string(relative)),
            None => js_string(&part.text),
        })
        .collect();
    format!("[{}]", parts.join(", "))
}

fn map_literal(map: &std::collections::BTreeMap<String, String>) -> String {
    let fields: Vec<String> = map
        .iter()
        .map(|(key, value)| format!("{}: {}", js_key(key), js_string(value)))
        .collect();
    format!("{{ {} }}", fields.join(", "))
}

fn permissions_literal(rules: &[crate::opencode::agents::PermissionRule]) -> String {
    let items: Vec<String> = rules
        .iter()
        .map(|rule| {
            format!(
                "{{ action: {}, resource: {}, effect: {} }}",
                js_string(&rule.action),
                js_string(&rule.resource),
                js_string(&rule.effect)
            )
        })
        .collect();
    format!("[{}]", items.join(", "))
}

fn js_string(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_string())
}

fn js_key(value: &str) -> String {
    if value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_')
        && value
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_alphabetic() || character == '_')
    {
        value.to_string()
    } else {
        js_string(value)
    }
}
