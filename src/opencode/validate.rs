use std::path::Path;

pub fn validate(dir: &Path) -> (Vec<String>, Vec<String>) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    let package_path = dir.join("package.json");
    let index_path = dir.join("src").join("index.ts");
    if !package_path.is_file() {
        errors.push("missing package.json".to_string());
    }
    if !index_path.is_file() {
        errors.push("missing src/index.ts".to_string());
    }
    if dir.join("extensions").is_dir() {
        warnings
            .push("extensions/ sidecar holds Claude data OpenCode v2 does not load".to_string());
    }

    if package_path.is_file() {
        match std::fs::read_to_string(&package_path) {
            Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(package) => validate_package(&package, &mut errors),
                Err(err) => errors.push(format!("package.json is not valid JSON: {err}")),
            },
            Err(err) => errors.push(format!("package.json is unreadable: {err}")),
        }
    }

    if index_path.is_file() {
        match std::fs::read_to_string(&index_path) {
            Ok(source) => validate_source(&source, &mut errors),
            Err(err) => errors.push(format!("src/index.ts is unreadable: {err}")),
        }
    }

    if let Ok(entries) = std::fs::read_dir(dir.join("skills")) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            let skill_md = path.join("SKILL.md");
            if !skill_md.is_file() {
                errors.push(format!("skills/{name} has no SKILL.md"));
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&skill_md) else {
                errors.push(format!("skills/{name}/SKILL.md is unreadable"));
                continue;
            };
            if !text.contains(&format!("name: {name}")) {
                errors.push(format!(
                    "skills/{name}/SKILL.md name does not match the directory"
                ));
            }
            if !text.contains("description:") {
                errors.push(format!("skills/{name}/SKILL.md is missing description"));
            }
        }
    }

    (errors, warnings)
}

fn validate_package(package: &serde_json::Value, errors: &mut Vec<String>) {
    if package
        .get("name")
        .and_then(|value| value.as_str())
        .is_none()
    {
        errors.push("package.json is missing name".to_string());
    }
    if package
        .get("version")
        .and_then(|value| value.as_str())
        .is_none()
    {
        errors.push("package.json is missing version".to_string());
    }
    if package.get("type").and_then(|value| value.as_str()) != Some("module") {
        errors.push("package.json type must be \"module\"".to_string());
    }
    match package
        .get("exports")
        .and_then(|value| value.get("."))
        .and_then(|value| value.as_str())
    {
        Some("./src/index.ts") => {}
        Some(other) => errors.push(format!(
            "package.json exports[\".\"] is '{other}', expected ./src/index.ts"
        )),
        None => errors.push("package.json is missing exports[\".\"]".to_string()),
    }
    if package
        .get("dependencies")
        .and_then(|value| value.get("@opencode/plugin"))
        .and_then(|value| value.as_str())
        .is_none()
    {
        errors.push("package.json is missing dependency @opencode/plugin".to_string());
    }
}

fn validate_source(source: &str, errors: &mut Vec<String>) {
    if !source.contains("Plugin.define") {
        errors.push("src/index.ts does not call Plugin.define".to_string());
    }
    if !source.contains("from \"@opencode/plugin\"") {
        errors.push("src/index.ts does not import @opencode/plugin".to_string());
    }
    if !source.contains("async setup") {
        errors.push("src/index.ts does not define setup".to_string());
    }
    if source.contains("ctx.skill.transform") && !source.contains("editor.add") {
        errors.push("src/index.ts skill transform does not call editor.add".to_string());
    }
    if source.contains("ctx.command.transform") && !source.contains("editor.add") {
        errors.push("src/index.ts command transform does not call editor.add".to_string());
    }
    if source.contains("ctx.mcp.transform") && !source.contains("editor.set") {
        errors.push("src/index.ts mcp transform does not call editor.set".to_string());
    }
    if source.contains("ctx.agent.transform") && !source.contains("editor.update") {
        errors.push("src/index.ts agent transform does not call editor.update".to_string());
    }
}
