# ARCHITECTURE.md

## Project Structure

```
src/
├── main.rs                 # Entry point
├── lib.rs                  # Library root (run function)
├── cli.rs                  # clap CLI definitions (Cli, Command enums)
├── error.rs                # thiserror error types
├── claude/                 # SOURCE format parsers
│   ├── manifest.rs         # .claude-plugin/plugin.json → ClaudeManifest
│   ├── mcp.rs              # .mcp.json → ClaudeMcpConfig
│   ├── skill.rs            # skills/*/SKILL.md → Skill (YAML frontmatter)
│   └── marketplace.rs      # .claude-plugin/marketplace.json → Marketplace
├── agent_plugins/          # TARGET format generators
│   ├── manifest.rs         # AgentManifest → plugin.json
│   ├── mcp.rs              # AgentMcpConfig → mcp.json
│   ├── skill.rs            # AgentSkill → SKILL.md
│   └── extensions.rs       # Extension data collection
├── convert/                # Conversion orchestrator
│   ├── mod.rs              # convert_single, convert_directory, data types
│   ├── manifest.rs         # ClaudeManifest → AgentManifest
│   ├── mcp.rs              # ClaudeMcpConfig → AgentMcpConfig
│   ├── skills.rs           # skills/ → skills/ with validation
│   ├── extensions.rs       # Non-portable components → extensions namespace
│   └── env_vars.rs         # Env var substitution
├── marketplace/            # Marketplace batch mode
│   ├── mod.rs              # convert_marketplace
│   ├── clone.rs            # git clone marketplace repo
│   └── convert.rs          # Batch convert all plugins
└── validate/               # Validation rules
    ├── name.rs             # Agent Plugins name constraints
    ├── skill.rs            # Skill frontmatter validation
    └── mcp.rs              # MCP config validation
```

## Data Flow

```
User CLI input
    │
    ▼
cli.rs (Clap parsing)
    │
    ▼
lib.rs::run() → Cli::execute()
    │
    ├── Command::Convert → convert::convert_single()
    ├── Command::ConvertDir → convert::convert_directory()
    └── Command::ConvertMarketplace → marketplace::convert_marketplace()
                                          │
                                          ▼
                                    marketplace::clone::clone_repo()
                                          │
                                          ▼
                                    marketplace::convert::batch_convert()
                                          │
                                    For each plugin:
                                          ▼
                                    convert::convert_single()
                                          │
                    ┌─────────────────────┼─────────────────────┐
                    ▼                     ▼                     ▼
            claude::manifest     claude::skill::parse_all   claude::mcp::parse
            ::parse()                                      (optional)
                    │                     │                     │
                    ▼                     ▼                     ▼
            convert::manifest    convert::skills::convert  convert::mcp::convert
            ::convert()          (writes skills/)          (writes mcp.json)
                    │
                    ▼
            agent_plugins::manifest::write()  →  plugin.json
                    │
                    ▼
            convert::extensions::collect() + write()
```

## Key Design Decisions

1. **Lossless conversion**: All Claude-specific data is preserved in `extensions`
2. **Git clone for marketplaces**: Default approach for marketplace source
3. **Configurable namespace**: Default `com.claude.code`, overridable via `--extension-namespace`
4. **Stdio default**: Claude MCP servers are implicit stdio; Agent Plugins requires explicit `type: "stdio"`
5. **Name normalization**: Claude plugin names are auto-normalized (lowercase, hyphens for spaces/underscores)
