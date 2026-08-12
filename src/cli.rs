use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::validate::name;

#[derive(Parser)]
#[command(
    name = "c2ap",
    about = "Convert Claude Code plugins to Agent Plugins v1.0.0"
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
    pub fn execute(&self) -> crate::error::Result<()> {
        if self.dry_run {
            println!(
                "[dry-run] Would convert with extension namespace '{}'",
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
            }
            return Ok(());
        }

        match &self.command {
            Command::Convert { path, output } => {
                if self.verbose {
                    eprintln!("Converting single plugin: {}", path.display());
                }
                crate::convert::convert_single(
                    path,
                    output,
                    &self.extension_namespace,
                    self.strict,
                )?;
                if !self.quiet {
                    println!("Converted plugin to {}", output.display());
                }
            }
            Command::ConvertDir { path, output } => {
                if self.verbose {
                    eprintln!("Converting plugins in directory: {}", path.display());
                }
                crate::convert::convert_directory(
                    path,
                    output,
                    &self.extension_namespace,
                    self.strict,
                )?;
                if !self.quiet {
                    println!("Converted plugins to {}", output.display());
                }
            }
            Command::ConvertMarketplace {
                repo,
                branch,
                output,
            } => {
                if self.verbose {
                    eprintln!("Converting marketplace: {} (branch: {})", repo, branch);
                }
                crate::marketplace::convert_marketplace(
                    repo,
                    branch,
                    output,
                    &self.extension_namespace,
                    self.strict,
                )?;
                if !self.quiet {
                    println!("Converted marketplace to {}", output.display());
                }
            }
        }
        Ok(())
    }
}
