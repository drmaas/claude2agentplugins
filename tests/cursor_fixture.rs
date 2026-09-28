use std::path::PathBuf;

use c2ap::convert::{ConvertOptions, Target, convert_directory, convert_single};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample-claude-plugin")
}

fn cursor_options() -> ConvertOptions {
    ConvertOptions {
        target: Target::Cursor,
        ..ConvertOptions::default()
    }
}

#[test]
fn converts_fixture_to_cursor_plugin() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("out");
    let report = convert_single(&fixture(), &output, &cursor_options()).unwrap();

    assert_eq!(report.name, "sample-plugin");
    assert_eq!(report.target, Target::Cursor);
    assert_eq!(report.skills_converted, 1);
    assert_eq!(report.commands_converted, 1);
    assert_eq!(report.mcp_servers, 2);
    assert!(!output.join("plugin.json").exists());

    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(output.join(".cursor-plugin/plugin.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(manifest["name"], "sample-plugin");
    assert_eq!(manifest["version"], "1.2.0");
    assert_eq!(manifest["author"]["name"], "Ada");
    assert!(manifest.get("displayName").is_none());
    assert_eq!(
        manifest["variables"]["properties"]["api_token"]["type"],
        "string"
    );
    assert_eq!(manifest["variables"]["required"][0], "api_token");

    let skill = std::fs::read_to_string(output.join("skills/review/SKILL.md")).unwrap();
    assert!(skill.contains("name: review"));
    assert!(skill.contains("${api_token}"));
    assert!(output.join("skills/review/scripts/check.sh").is_file());

    let command = std::fs::read_to_string(output.join("commands/deploy.md")).unwrap();
    assert!(command.contains("name: deploy"));
    assert!(command.contains("Deploy the app"));
    assert!(!command.contains("argument-hint"));

    let agent = std::fs::read_to_string(output.join("agents/security-reviewer.md")).unwrap();
    assert!(agent.contains("name: security-reviewer"));
    assert!(agent.contains("Be careful."));
    assert!(agent.contains("model: inherit[effort=high]"));
    assert!(agent.contains("is_background: true"));
    assert!(agent.contains("readonly: true"));
    assert!(!agent.contains("tools:"));
    assert!(!agent.contains("sonnet"));

    let rule = std::fs::read_to_string(output.join("rules/prefer-const.mdc")).unwrap();
    assert!(rule.contains("globs:"));
    assert!(rule.contains("**/*.ts"));
    assert!(!rule.contains("paths:"));
    let claude = std::fs::read_to_string(output.join("rules/claude.mdc")).unwrap();
    assert!(claude.contains("alwaysApply: true"));
    assert!(claude.contains("Follow the house style."));

    let hooks: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(output.join("hooks/hooks.json")).unwrap())
            .unwrap();
    assert_eq!(
        hooks["hooks"]["preToolUse"][0]["command"],
        "./scripts/c2ap-prepend-bin-path.py"
    );
    assert_eq!(hooks["hooks"]["preToolUse"][0]["matcher"], "Shell");
    assert_eq!(
        hooks["hooks"]["preToolUse"][1]["command"],
        "./scripts/check.sh"
    );
    assert_eq!(hooks["hooks"]["preToolUse"][1]["matcher"], "Shell|Write");
    assert!(hooks["hooks"].get("Notification").is_none());
    assert!(hooks["hooks"].get("beforeShellExecution").is_none());
    assert!(output.join("scripts/c2ap-prepend-bin-path.py").is_file());

    let mcp: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(output.join("mcp.json")).unwrap()).unwrap();
    assert_eq!(
        mcp["mcpServers"]["local"]["command"],
        "${CURSOR_PLUGIN_ROOT}/bin/server"
    );
    assert_eq!(mcp["mcpServers"]["local"]["env"]["TOKEN"], "${api_token}");
    assert_eq!(
        mcp["mcpServers"]["remote"]["url"],
        "https://example.com/mcp"
    );
    assert!(mcp["mcpServers"].get("socket").is_none());
    assert!(mcp.get("$schema").is_none());

    let sidecar = output.join("com.claude.code");
    assert!(sidecar.join("plugin.json").is_file());
    assert!(sidecar.join(".mcp.json").is_file());
    assert!(sidecar.join(".lsp.json").is_file());
    assert!(sidecar.join("settings.json").is_file());
    assert!(sidecar.join("output-styles/terse.md").is_file());
    assert!(sidecar.join("manifest-extras.json").is_file());
    assert!(sidecar.join("hooks-original/hooks.json").is_file());
    assert!(output.join("scripts/check.sh").is_file());
    assert!(output.join("bin/server").is_file());
    assert_eq!(
        std::fs::read_to_string(output.join("README.md")).unwrap(),
        "# Sample plugin\n\nFixture for the Cursor conversion target.\n"
    );

    let warning_text = report.warnings.join("\n");
    for needle in [
        "displayName",
        "author.url",
        "ws",
        "Notification",
        "argument-hint",
        "dashboard",
        "no official rules",
        "bin/",
        "PATH",
        ".lsp.json",
        "settings.json",
        "subagentStatusLine",
        "agent",
        "output-styles",
        "is_background",
        "readonly",
    ] {
        assert!(
            warning_text.contains(needle),
            "missing warning containing '{needle}':\n{warning_text}"
        );
    }

    let (errors, validation_warnings) = c2ap::cursor::validate::validate_plugin(&output);
    assert!(errors.is_empty(), "{errors:?}");
    assert!(validation_warnings.is_empty(), "{validation_warnings:?}");
}

#[test]
fn fixture_still_converts_to_agent_plugins() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("out");
    let report = convert_single(&fixture(), &output, &ConvertOptions::default()).unwrap();
    assert_eq!(report.target, Target::AgentPlugins);
    assert!(output.join("plugin.json").is_file());
    assert!(!output.join(".cursor-plugin").exists());
    assert!(output.join("skills/review/SKILL.md").is_file());
    assert!(
        output
            .join("com.claude.code/agents/security-reviewer.md")
            .is_file()
    );
    let mcp = std::fs::read_to_string(output.join("mcp.json")).unwrap();
    assert!(mcp.contains("\"type\": \"stdio\"") || mcp.contains("\"type\":\"stdio\""));
    assert!(mcp.contains("streamable-http"));
}

#[test]
fn convert_directory_writes_cursor_marketplace() {
    let temp = tempfile::tempdir().unwrap();
    let plugins = temp.path().join("plugins");
    let plugin = plugins.join("sample-plugin");
    copy_dir(&fixture(), &plugin);
    let output = temp.path().join("out");
    let report = convert_directory(&plugins, &output, &cursor_options()).unwrap();
    assert_eq!(report.plugins.len(), 1);
    let marketplace =
        std::fs::read_to_string(output.join(".cursor-plugin/marketplace.json")).unwrap();
    let value: serde_json::Value = serde_json::from_str(&marketplace).unwrap();
    assert_eq!(value["plugins"][0]["name"], "sample-plugin");
    assert_eq!(value["plugins"][0]["source"], "sample-plugin");
    assert!(
        output
            .join("sample-plugin/.cursor-plugin/plugin.json")
            .is_file()
    );
}

fn copy_dir(src: &std::path::Path, dst: &std::path::Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let dest = dst.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &dest);
        } else {
            std::fs::copy(entry.path(), dest).unwrap();
        }
    }
}
