use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

use crate::convert::{ConversionReport, ConvertOptions, DirectoryReport, Target};
use crate::error::Result;
use crate::validate::name;

#[derive(Parser)]
#[command(
    name = "c2ap",
    about = "Convert Claude Code plugins to Agent Plugins or Cursor plugins"
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
        }
        Ok(())
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
