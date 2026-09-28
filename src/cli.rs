use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

use crate::convert::{ConversionReport, ConvertOptions, DirectoryReport, Target};
use crate::error::Result;
use crate::validate::name;

#[derive(Parser)]
#[command(
    name = "c2ap",
    about = "Convert Claude Code plugins to Agent Plugins, Cursor plugins, or OpenCode v2 plugins"
)]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    #[arg(
        short,
        long,
        default_value = "com.claude.code",
        global = true,
        help = "Extension namespace for Claude-specific data",
        value_parser = validate_extension_namespace
    )]
    pub extension_namespace: String,

    #[arg(
        short,
        long,
        default_value = "false",
        global = true,
        help = "Fail on any warning"
    )]
    pub strict: bool,

    #[arg(
        short = 'n',
        long,
        default_value = "false",
        global = true,
        help = "Show what would be done without writing"
    )]
    pub dry_run: bool,

    #[arg(
        long,
        global = true,
        help = "Convert commands/ to skills. Cursor skills get disable-model-invocation: true"
    )]
    pub convert_commands: bool,

    #[arg(
        long,
        global = true,
        value_enum,
        default_value = "agent-plugins",
        help = "Conversion target (agent-plugins or cursor)"
    )]
    pub target: Target,

    #[arg(long, global = true, help = "Overwrite non-empty output directories")]
    pub force: bool,

    #[arg(long, global = true, help = "Emit a machine-readable JSON summary")]
    pub json: bool,

    #[arg(short, long, global = true, help = "Verbose output")]
    pub verbose: bool,

    #[arg(short, long, global = true, help = "Suppress output except errors")]
    pub quiet: bool,
}

#[derive(Subcommand)]
pub enum Command {
    Convert {
        path: PathBuf,

        #[arg(short, long, default_value = "./output")]
        output: PathBuf,
    },
    ConvertDir {
        path: PathBuf,

        #[arg(short, long, default_value = "./output")]
        output: PathBuf,
    },
    ConvertMarketplace {
        #[arg(long)]
        repo: String,

        #[arg(long, default_value = "main")]
        branch: String,

        #[arg(short, long, default_value = "./output")]
        output: PathBuf,
    },
    Validate {
        path: PathBuf,
    },
    Init {
        name: String,

        #[arg(short, long, default_value = "./")]
        output: PathBuf,

        #[arg(long, default_value = "A new Agent Plugins plugin")]
        description: String,
    },
    /// Convert one Claude plugin into an OpenCode v2 plugin package
    ConvertOpenCode {
        path: PathBuf,

        #[arg(short, long, default_value = "./output")]
        output: PathBuf,
    },
    /// Convert each Claude plugin in a directory into an OpenCode v2 plugin package
    ConvertOpenCodeDir {
        path: PathBuf,

        #[arg(short, long, default_value = "./output")]
        output: PathBuf,
    },
    /// Convert a Claude marketplace into OpenCode v2 plugin packages
    ConvertOpenCodeMarketplace {
        #[arg(long)]
        repo: String,

        #[arg(long, default_value = "main")]
        branch: String,

        #[arg(short, long, default_value = "./output")]
        output: PathBuf,
    },
    /// Validate an OpenCode v2 plugin package produced by c2ap
    ValidateOpenCode {
        path: PathBuf,
    },
}

fn validate_extension_namespace(s: &str) -> std::result::Result<String, String> {
    if name::is_valid_extension_namespace(s) {
        Ok(s.to_string())
    } else {
        Err(format!(
            "Invalid extension namespace '{}': must be a valid reverse-domain name (e.g. com.example.app)",
            s
        ))
    }
}

impl Cli {
    fn options(&self) -> ConvertOptions {
        ConvertOptions {
            extension_namespace: self.extension_namespace.clone(),
            strict: self.strict,
            convert_commands: self.convert_commands,
            force: self.force,
            preferred_name: None,
            target: self.target,
        }
    }

    pub fn execute(&self) -> Result<()> {
        if self.dry_run {
            return self.dry_run_print();
        }

        match &self.command {
            Command::Convert { path, output } => {
                if self.verbose {
                    eprintln!("Converting single plugin: {}", path.display());
                }
                let report = crate::convert::convert_single(path, output, &self.options())?;
                self.print_report(&report);
            }
            Command::ConvertDir { path, output } => {
                if self.verbose {
                    eprintln!("Converting plugins in directory: {}", path.display());
                }
                let report = crate::convert::convert_directory(path, output, &self.options())?;
                self.print_directory_report(&report);
            }
            Command::ConvertMarketplace {
                repo,
                branch,
                output,
            } => {
                if self.verbose {
                    eprintln!("Converting marketplace: {} (branch: {})", repo, branch);
                }
                let report =
                    crate::marketplace::convert_marketplace(repo, branch, output, &self.options())?;
                self.print_directory_report(&report);
            }
            Command::Validate { path } => {
                let target = self.validate_target(path);
                let (errors, warnings) = match target {
                    Target::Cursor => crate::cursor::validate::validate_plugin(path),
                    Target::AgentPlugins => crate::validate::plugin::validate_plugin(path),
                };
                for w in &warnings {
                    eprintln!("Warning: {}", w);
                }
                for e in &errors {
                    eprintln!("Error: {}", e);
                }
                if self.json {
                    let value = serde_json::json!({
                        "path": path.display().to_string(),
                        "errors": errors,
                        "warnings": warnings,
                    });
                    println!("{}", serde_json::to_string_pretty(&value).unwrap());
                } else {
                    println!(
                        "Validated {}: {} error(s), {} warning(s)",
                        path.display(),
                        errors.len(),
                        warnings.len()
                    );
                }
                if !errors.is_empty() {
                    return Err(crate::error::Error::Validation {
                        message: format!("{} validation error(s) found", errors.len()),
                        source: None,
                    });
                }
            }
            Command::Init {
                name,
                output,
                description,
            } => {
                self.init_plugin(name, output.as_path(), description)?;
            }
            Command::ConvertOpenCode { path, output } => {
                if self.verbose {
                    eprintln!(
                        "Converting single plugin to OpenCode v2: {}",
                        path.display()
                    );
                }
                let report =
                    crate::opencode::convert_single(path, output, &self.opencode_options())?;
                self.print_opencode_report(&report);
            }
            Command::ConvertOpenCodeDir { path, output } => {
                if self.verbose {
                    eprintln!(
                        "Converting plugins in directory to OpenCode v2: {}",
                        path.display()
                    );
                }
                let report =
                    crate::opencode::convert_directory(path, output, &self.opencode_options())?;
                self.print_opencode_directory_report(&report);
            }
            Command::ConvertOpenCodeMarketplace {
                repo,
                branch,
                output,
            } => {
                if self.verbose {
                    eprintln!(
                        "Converting marketplace to OpenCode v2: {} (branch: {})",
                        repo, branch
                    );
                }
                let report = crate::opencode::marketplace::convert_marketplace(
                    repo,
                    branch,
                    output,
                    &self.opencode_options(),
                )?;
                self.print_opencode_directory_report(&report);
            }
            Command::ValidateOpenCode { path } => {
                let (errors, warnings) = crate::opencode::validate::validate(path);
                for warning in &warnings {
                    eprintln!("Warning: {warning}");
                }
                for error in &errors {
                    eprintln!("Error: {error}");
                }
                if self.json {
                    let value = serde_json::json!({
                        "path": path.display().to_string(),
                        "errors": errors,
                        "warnings": warnings,
                    });
                    println!("{}", serde_json::to_string_pretty(&value).unwrap());
                } else {
                    println!(
                        "Validated {}: {} error(s), {} warning(s)",
                        path.display(),
                        errors.len(),
                        warnings.len()
                    );
                }
                if !errors.is_empty() {
                    return Err(crate::error::Error::Validation {
                        message: format!("{} validation error(s) found", errors.len()),
                        source: None,
                    });
                }
            }
        }
        Ok(())
    }

    fn opencode_options(&self) -> crate::opencode::OpenCodeOptions {
        crate::opencode::OpenCodeOptions {
            extension_namespace: self.extension_namespace.clone(),
            strict: self.strict,
            force: self.force,
            preferred_name: None,
        }
    }

    fn dry_run_print(&self) -> Result<()> {
        println!(
            "[dry-run] Would convert with target '{}' and extension namespace '{}'",
            self.target.as_str(),
            self.extension_namespace
        );
        match &self.command {
            Command::Convert { path, output } => {
                println!(
                    "[dry-run] convert {} -> {}",
                    path.display(),
                    output.display()
                );
            }
            Command::ConvertDir { path, output } => {
                println!(
                    "[dry-run] convert-dir {} -> {}",
                    path.display(),
                    output.display()
                );
            }
            Command::ConvertMarketplace {
                repo,
                branch,
                output,
            } => {
                println!(
                    "[dry-run] convert-marketplace --repo {} --branch {} -> {}",
                    repo,
                    branch,
                    output.display()
                );
            }
            Command::Validate { path } => {
                println!("[dry-run] validate {}", path.display());
            }
            Command::Init { name, output, .. } => {
                println!("[dry-run] init {} -> {}", name, output.display());
            }
            Command::ConvertOpenCode { path, output } => {
                println!(
                    "[dry-run] convert-opencode {} -> {}",
                    path.display(),
                    output.display()
                );
            }
            Command::ConvertOpenCodeDir { path, output } => {
                println!(
                    "[dry-run] convert-opencode-dir {} -> {}",
                    path.display(),
                    output.display()
                );
            }
            Command::ConvertOpenCodeMarketplace {
                repo,
                branch,
                output,
            } => {
                println!(
                    "[dry-run] convert-opencode-marketplace --repo {} --branch {} -> {}",
                    repo,
                    branch,
                    output.display()
                );
            }
            Command::ValidateOpenCode { path } => {
                println!("[dry-run] validate-opencode {}", path.display());
            }
        }
        Ok(())
    }

    fn print_report(&self, report: &ConversionReport) {
        for w in &report.warnings {
            eprintln!("Warning: {}", w);
        }
        if self.json {
            let value = serde_json::json!({
                "name": report.name,
                "target": report.target.as_str(),
                "output": report.output.display().to_string(),
                "manifestSynthesized": report.manifest_synthesized,
                "skills": report.skills_converted,
                "commands": report.commands_converted,
                "mcpServers": report.mcp_servers,
                "extensions": report.extension_dirs,
                "warnings": report.warnings,
            });
            println!("{}", serde_json::to_string_pretty(&value).unwrap());
        } else if !self.quiet && report.target == Target::Cursor {
            println!(
                "Converted '{}' to Cursor plugin at {} ({} skills, {} commands, {} mcp servers, {} sidecar entries{})",
                report.name,
                report.output.display(),
                report.skills_converted,
                report.commands_converted,
                report.mcp_servers,
                report.extension_dirs,
                if report.manifest_synthesized {
                    ", synthesized manifest"
                } else {
                    ""
                }
            );
        } else if !self.quiet {
            println!(
                "Converted '{}' to {} ({} skills, {} mcp servers, {} extensions{}{})",
                report.name,
                report.output.display(),
                report.skills_converted,
                report.mcp_servers,
                report.extension_dirs,
                if report.commands_converted > 0 {
                    format!(", {} commands", report.commands_converted)
                } else {
                    String::new()
                },
                if report.manifest_synthesized {
                    ", synthesized manifest"
                } else {
                    ""
                }
            );
        }
    }

    fn print_directory_report(&self, report: &DirectoryReport) {
        for plugin in &report.plugins {
            for w in &plugin.warnings {
                eprintln!("Warning: {}", w);
            }
        }
        for s in &report.skipped {
            eprintln!("Skipped: {}", s);
        }
        if self.json {
            let value = serde_json::json!({
                "converted": report.plugins.len(),
                "skipped": report.skipped.len(),
                "plugins": report.plugins.iter().map(|p| {
                    serde_json::json!({
                        "name": p.name,
                        "target": p.target.as_str(),
                        "output": p.output.display().to_string(),
                        "skills": p.skills_converted,
                        "commands": p.commands_converted,
                        "mcpServers": p.mcp_servers,
                        "extensions": p.extension_dirs,
                        "warnings": p.warnings,
                    })
                }).collect::<Vec<_>>(),
                "skippedItems": report.skipped,
            });
            println!("{}", serde_json::to_string_pretty(&value).unwrap());
        } else if !self.quiet {
            println!(
                "Converted {} plugin(s), skipped {}",
                report.plugins.len(),
                report.skipped.len()
            );
        }
    }

    fn print_opencode_report(&self, report: &crate::opencode::OpenCodeReport) {
        for warning in &report.warnings {
            eprintln!("Warning: {warning}");
        }
        if self.json {
            let value = serde_json::json!({
                "name": report.name,
                "output": report.output.display().to_string(),
                "manifestSynthesized": report.manifest_synthesized,
                "skills": report.skills_converted,
                "agents": report.agents_converted,
                "commands": report.commands_converted,
                "mcpServers": report.mcp_servers,
                "sidecarEntries": report.sidecar_entries,
                "warnings": report.warnings,
            });
            println!("{}", serde_json::to_string_pretty(&value).unwrap());
        } else if !self.quiet {
            println!(
                "Converted '{}' to OpenCode v2 plugin {} ({} skills, {} agents, {} commands, {} mcp servers, {} sidecar entries{})",
                report.name,
                report.output.display(),
                report.skills_converted,
                report.agents_converted,
                report.commands_converted,
                report.mcp_servers,
                report.sidecar_entries,
                if report.manifest_synthesized {
                    ", synthesized manifest"
                } else {
                    ""
                }
            );
        }
    }

    fn print_opencode_directory_report(&self, report: &crate::opencode::OpenCodeDirectoryReport) {
        for plugin in &report.plugins {
            for warning in &plugin.warnings {
                eprintln!("Warning: {warning}");
            }
        }
        for skipped in &report.skipped {
            eprintln!("Skipped: {skipped}");
        }
        if self.json {
            let value = serde_json::json!({
                "converted": report.plugins.len(),
                "skipped": report.skipped.len(),
                "plugins": report.plugins.iter().map(|plugin| {
                    serde_json::json!({
                        "name": plugin.name,
                        "output": plugin.output.display().to_string(),
                        "skills": plugin.skills_converted,
                        "agents": plugin.agents_converted,
                        "commands": plugin.commands_converted,
                        "mcpServers": plugin.mcp_servers,
                        "sidecarEntries": plugin.sidecar_entries,
                        "warnings": plugin.warnings,
                    })
                }).collect::<Vec<_>>(),
                "skippedItems": report.skipped,
            });
            println!("{}", serde_json::to_string_pretty(&value).unwrap());
        } else if !self.quiet {
            println!(
                "Converted {} OpenCode v2 plugin(s), skipped {}",
                report.plugins.len(),
                report.skipped.len()
            );
        }
    }

    fn init_plugin(&self, name: &str, output: &Path, description: &str) -> Result<()> {
        if !name::is_valid_plugin_name(name) {
            return Err(crate::error::Error::Validation {
                message: format!(
                    "Invalid plugin name '{}': must be 1-64 chars of lowercase letters, digits, hyphens and periods (no '--' or '..', must start and end alphanumeric)",
                    name
                ),
                source: None,
            });
        }
        let plugin_dir = output.join(name);
        if plugin_dir.exists() && !self.force {
            return Err(crate::error::Error::Conversion(format!(
                "Target '{}' already exists; use --force to overwrite",
                plugin_dir.display()
            )));
        }
        if self.target == Target::Cursor {
            return self.init_cursor_plugin(&plugin_dir, name, description);
        }
        std::fs::create_dir_all(plugin_dir.join("skills").join(name))?;

        let manifest = serde_json::json!({
            "$schema": crate::agent_plugins::manifest::PLUGIN_SCHEMA,
            "name": name,
            "version": "0.1.0",
            "description": description,
            "keywords": [],
        });
        std::fs::write(
            plugin_dir.join("plugin.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )?;

        let skill_md = format!(
            "---\nname: {}\ndescription: {}\n---\n\nWrite instructions for this skill.\n",
            name, description
        );
        std::fs::write(
            plugin_dir.join("skills").join(name).join("SKILL.md"),
            skill_md,
        )?;

        if !self.quiet {
            println!("Created Agent Plugins plugin at {}", plugin_dir.display());
        }
        Ok(())
    }

    fn validate_target(&self, path: &Path) -> Target {
        if self.target == Target::Cursor {
            return Target::Cursor;
        }
        if path.join(".cursor-plugin").join("plugin.json").is_file()
            && !path.join("plugin.json").is_file()
        {
            return Target::Cursor;
        }
        Target::AgentPlugins
    }

    fn init_cursor_plugin(&self, plugin_dir: &Path, name: &str, description: &str) -> Result<()> {
        std::fs::create_dir_all(plugin_dir.join(".cursor-plugin"))?;
        std::fs::create_dir_all(plugin_dir.join("skills").join(name))?;
        let manifest = serde_json::json!({
            "name": name,
            "version": "0.1.0",
            "description": description,
        });
        std::fs::write(
            plugin_dir.join(".cursor-plugin").join("plugin.json"),
            serde_json::to_string_pretty(&manifest).unwrap(),
        )?;
        let skill_md = format!(
            "---\nname: {name}\ndescription: {description}\n---\n\nWrite instructions for this skill.\n"
        );
        std::fs::write(
            plugin_dir.join("skills").join(name).join("SKILL.md"),
            skill_md,
        )?;
        if !self.quiet {
            println!("Created Cursor plugin at {}", plugin_dir.display());
        }
        Ok(())
    }
}
