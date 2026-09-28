use std::path::PathBuf;

use c2ap::convert::{ConvertOptions, convert_single};
use c2ap::opencode::marketplace::convert_marketplace;
use c2ap::opencode::validate::validate;
use c2ap::opencode::{OpenCodeOptions, convert_directory, convert_single as convert_opencode};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_plugin")
}

#[test]
fn converts_claude_plugin_to_opencode_v2_package() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("sample");
    let report = convert_opencode(&fixture(), &output, &OpenCodeOptions::default()).unwrap();

    assert_eq!(report.name, "Sample Plugin");
    assert_eq!(report.skills_converted, 1);
    assert_eq!(report.agents_converted, 1);
    assert_eq!(report.commands_converted, 1);
    assert_eq!(report.mcp_servers, 2);
    assert!(report.sidecar_entries > 0);
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("hooks"))
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("'ws'"))
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("SSE"))
    );

    let package: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(output.join("package.json")).unwrap())
            .unwrap();
    assert_eq!(package["name"], "opencode-sample-plugin");
    assert_eq!(package["version"], "1.2.3");
    assert_eq!(package["type"], "module");
    assert_eq!(package["exports"]["."], "./src/index.ts");
    assert!(package["dependencies"]["@opencode/plugin"].is_string());

    let source = std::fs::read_to_string(output.join("src/index.ts")).unwrap();
    assert!(source.contains("import { Plugin } from \"@opencode/plugin\""));
    assert!(source.contains("Plugin.define"));
    assert!(source.contains("id: \"sample-plugin\""));
    assert!(source.contains("ctx.skill.transform"));
    assert!(source.contains("editor.add"));
    assert!(source.contains("id: \"review\""));
    assert!(source.contains("ctx.agent.transform"));
    assert!(source.contains("editor.update(\"reviewer\""));
    assert!(source.contains("agent.mode = \"subagent\""));
    assert!(source.contains("agent.system = \"You review the diff.\""));
    assert!(source.contains("effect: \"allow\""));
    assert!(source.contains("ctx.command.transform"));
    assert!(source.contains("name: \"ship\""));
    assert!(source.contains("ctx.session.prompt"));
    assert!(source.contains("replaceAll(\"$ARGUMENTS\", args)"));
    assert!(source.contains("ctx.mcp.transform"));
    assert!(source.contains("editor.set(\"local\""));
    assert!(source.contains("type: \"local\""));
    assert!(source.contains("path.join(root, \"servers/index.js\")"));
    assert!(source.contains("environment: { NODE_ENV: \"production\" }"));
    assert!(source.contains("editor.set(\"docs\""));
    assert!(source.contains("type: \"remote\""));
    assert!(!source.contains("wss://example.com/mcp"));
    assert!(!output.join("opencode.json").exists());
    assert!(!output.join("plugin.json").exists());

    let skill = std::fs::read_to_string(output.join("skills/review/SKILL.md")).unwrap();
    assert!(skill.contains("name: review"));
    assert!(skill.contains("Review the current changes for correctness"));
    assert!(output.join("servers/index.js").is_file());
    assert!(
        output
            .join("extensions/com.claude.code/hooks/hooks.json")
            .is_file()
    );

    let (errors, warnings) = validate(&output);
    assert!(errors.is_empty(), "{errors:?}");
    assert!(!warnings.is_empty());
}

#[test]
fn strict_mode_fails_when_features_are_unmapped() {
    let temp = tempfile::tempdir().unwrap();
    let options = OpenCodeOptions {
        strict: true,
        ..OpenCodeOptions::default()
    };
    let error = convert_opencode(&fixture(), &temp.path().join("out"), &options).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("strict"), "{message}");
}

#[test]
fn agent_plugins_conversion_still_writes_plugin_json() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("agent");
    let report = convert_single(&fixture(), &output, &ConvertOptions::default()).unwrap();
    assert!(output.join("plugin.json").is_file());
    assert!(report.skills_converted >= 1);
    assert!(!output.join("src/index.ts").exists());
}

#[test]
fn converts_a_directory_and_a_local_marketplace() {
    let temp = tempfile::tempdir().unwrap();
    let plugins = temp.path().join("plugins");
    let sample = plugins.join("sample-plugin");
    copy_dir(&fixture(), &sample);

    let dir_out = temp.path().join("dir-out");
    let directory = convert_directory(&plugins, &dir_out, &OpenCodeOptions::default()).unwrap();
    assert_eq!(directory.plugins.len(), 1);
    assert!(dir_out.join("sample-plugin/package.json").is_file());

    let repo = temp.path().join("repo");
    std::fs::create_dir_all(repo.join(".claude-plugin")).unwrap();
    std::fs::write(
        repo.join(".claude-plugin").join("marketplace.json"),
        r#"{
            "name": "acme",
            "metadata": {"pluginRoot": "./plugins"},
            "plugins": [{"name": "sample-plugin", "source": "sample-plugin"}]
        }"#,
    )
    .unwrap();
    copy_dir(&fixture(), &repo.join("plugins").join("sample-plugin"));
    let market_out = temp.path().join("market-out");
    let market = convert_marketplace(
        repo.to_str().unwrap(),
        "main",
        &market_out,
        &OpenCodeOptions::default(),
    )
    .unwrap();
    assert_eq!(market.plugins.len(), 1);
    assert!(market_out.join("sample-plugin/src/index.ts").is_file());
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in walkdir::WalkDir::new(from) {
        let entry = entry.unwrap();
        let relative = entry.path().strip_prefix(from).unwrap();
        let dest = to.join(relative);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(dest).unwrap();
        } else {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::copy(entry.path(), dest).unwrap();
        }
    }
}
