# ARCHITECTURE.md

## Project Structure

```
src/
├── main.rs                 # Entry point
├── lib.rs                  # Library root (run function)
├── cli.rs                  # clap CLI definitions (Cli, Command enums, report printing)
├── error.rs                # thiserror error types
├── claude/                 # SOURCE format parsers
│   ├── manifest.rs         # .claude-plugin/plugin.json → ClaudeManifest (+ synthesize for manifest-less plugins)
│   ├── mcp.rs              # .mcp.json → ClaudeMcpConfig (stdio + remote http/sse/ws servers)
│   ├── skill.rs            # skills/*/SKILL.md → Skill; root SKILL.md; custom dirs; command files
│   └── marketplace.rs      # .claude-plugin/marketplace.json → Marketplace (modern object + legacy array)
├── agent_plugins/          # Agent Plugins target generators
│   ├── manifest.rs         # AgentManifest → plugin.json
│   ├── mcp.rs              # AgentMcpConfig → mcp.json
│   └── skill.rs            # AgentSkill → SKILL.md
├── cursor/                 # Cursor plugin target (`.cursor-plugin/plugin.json`)
│   ├── convert.rs          # Orchestrator; reuses claude parse/load
│   ├── manifest.rs         # Cursor manifest + userConfig → variables
│   ├── mcp.rs              # .mcp.json → mcp.json (`${CURSOR_PLUGIN_ROOT}`)
│   ├── skills.rs           # skills/*/SKILL.md (name matches folder)
│   ├── rules.rs            # rules/ and CLAUDE.md → rules/*.mdc
│   ├── agents.rs           # agents/*.md
│   ├── commands.rs         # commands/*.md, or skills with --convert-commands
│   ├── hooks.rs            # hooks/hooks.json event and tool mapping
│   ├── sidecar.rs          # unmapped Claude data → <namespace>/
│   ├── validate.rs         # Cursor output checks
│   └── marketplace_manifest.rs
├── convert/                # Agent Plugins orchestrator; dispatches --target cursor
│   ├── mod.rs              # convert_single / convert_directory, ConvertOptions, ConversionReport
│   ├── manifest.rs         # ClaudeManifest → AgentManifest
│   ├── mcp.rs              # ClaudeMcpConfig → AgentMcpConfig (command rewrite, remote servers)
│   ├── skills.rs           # skills/ → skills/ (validation, normalization, aux file copy); commands → skills
│   ├── extensions.rs       # Non-portable components → extension namespace (incl. custom paths)
│   └── env_vars.rs         # Placeholder transformation (${CLAUDE_*} → ${PLUGIN_*}, command rewrite)
├── marketplace/            # Marketplace batch mode
│   ├── mod.rs              # convert_marketplace (git URL, owner/repo, or local directory)
│   ├── clone.rs            # git clone with ref/sha pinning
│   └── convert.rs          # Batch convert all plugins (per-source fetch, collision detection)
├── opencode/               # OpenCode v2 plugin package target
│   ├── mod.rs              # convert_single / convert_directory
│   ├── render.rs           # package.json + src/index.ts (Plugin.define)
│   ├── skills.rs           # skills → editor.add + skills/<name>/SKILL.md
│   ├── agents.rs           # agents → editor.update (Agent.Info)
│   ├── commands.rs         # commands → editor.add execute → session.prompt
│   ├── hooks.rs            # Claude hooks → ctx.tool/session/shell/permission.hook
│   ├── mcp.rs              # .mcp.json → editor.set local/remote
│   ├── sidecar.rs          # unmapped Claude files → extensions/<namespace>/
│   ├── validate.rs         # OpenCode v2 package checks (`validate-opencode`)
│   └── marketplace.rs      # marketplace batch mode for the OpenCode target
└── validate/               # Validation rules
    ├── name.rs             # Plugin/skill name constraints and normalization
    ├── skill.rs            # Skill frontmatter validation
    ├── mcp.rs              # MCP config validation
    └── plugin.rs           # Whole-plugin conformance check (backs `c2ap validate` + self-validation)
```

## Data Flow

```
User CLI input
    │
    ▼
cli.rs (Clap parsing)
    │
    ├── Command::Convert         → convert::convert_single()
    ├── Command::ConvertDir      → convert::convert_directory()
    ├── Command::ConvertMarketplace → marketplace::convert_marketplace()
    ├── Command::Validate        → validate::plugin::validate_plugin()
    ├── Command::Init            → scaffold plugin.json + skills/<name>/SKILL.md
    ├── Command::ConvertOpenCode → opencode::convert_single()
    ├── Command::ConvertOpenCodeDir → opencode::convert_directory()
    ├── Command::ConvertOpenCodeMarketplace → opencode::marketplace::convert_marketplace()
    └── Command::ValidateOpenCode → opencode::validate::validate()

convert::convert_single(input, output, options)
    │
    ├── Target::Cursor → cursor::convert_single (same Claude parsers, Cursor layout)
    ├── Resolve manifest (.claude-plugin/plugin.json) or synthesize from dir name
    ├── Parse skills (skills/, custom paths from manifest, root SKILL.md fallback)
    ├── Parse + convert .mcp.json (stdio rewrite, remote http/sse, ws → warning)
    ├── Write plugin.json (with $schema + extensions)
    ├── Write skills/ (normalized names, aux files copied, allowed-tools as string)
    ├── Optionally convert commands/ → skills/ (--convert-commands)
    ├── Copy non-portable components → <extension-namespace>/ (default dirs + custom paths)
    ├── Copy LICENSE / README.md / CHANGELOG.md
    └── Self-validate output (validate::plugin) and merge findings into warnings
```

## Key Design Decisions

1. **Lossless conversion**: All Claude-specific data is preserved under the extension namespace — as files in a top-level `<namespace>/` directory (per Agent Plugins §8.2) and/or as manifest `extensions` data. This includes the original `.mcp.json`, `bin/`, `settings.json`, and custom component paths.
2. **Portable core = skills + MCP**: Only skills and MCP servers are portable in Agent Plugins v1. Everything else is parked in extensions, and warnings explain what moved where.
3. **Manifest-less support**: Claude's manifest is optional, so c2ap synthesizes one from the directory name and handles single-skill plugins with a root `SKILL.md`.
4. **Spec-valid output**: Commands are rewritten from `${CLAUDE_PLUGIN_ROOT}/...` to `./...` (Agent Plugins does not expand placeholders in `command`), `allowed-tools` is written as a space-separated string, metadata values are stringified, and skill names are normalized. A post-conversion self-check validates the output against the v1.0.0 rules.
5. **Configurable namespace**: Default `com.claude.code`, overridable via `--extension-namespace`.
6. **Marketplace formats**: Both the modern `{plugins: [...]}` object format (string sources, `metadata.pluginRoot`, `github`/`url`/`git-subdir`/`npm`/`archive` sources with ref/sha pins) and the legacy bare-array format are parsed. `npm` and `archive` sources use system tools (`npm pack` + `tar`, `curl` + `unzip`) when available.
7. **Idempotent output**: Non-empty output directories are refused unless `--force` is given; normalized name collisions in `convert-dir`/`convert-marketplace` are reported as errors.
8. **Stdio default**: Claude MCP servers are implicit stdio; Agent Plugins requires explicit `type: "stdio"`. Remote `http`/`sse` servers map to `streamable-http`/`sse`; `ws` servers have no equivalent and are warned about.
9. **Name normalization**: Claude plugin names are auto-normalized (lowercase, hyphens for spaces/underscores), and skill names are normalized to the stricter Agent Skills character set.
10. **OpenCode v2 is a separate target**: `convert-opencode` does not change the Agent Plugins or Cursor paths. It writes a v2 plugin package (`package.json` with `type: "module"` and `@opencode/plugin`, plus `src/index.ts` that default-exports `Plugin.define`). Skills, commands, agents, and MCP servers are registered in `setup` through `ctx.skill`, `ctx.command`, `ctx.agent`, and `ctx.mcp`. Claude command hooks map onto `ctx.tool.hook` / `ctx.session.hook` / `ctx.shell.hook` / `ctx.permission.hook` when semantics fit; unmapped hook events and non-command hook types stay in `extensions/<namespace>/` with warnings.
11. **Cursor is `--target cursor`**: `convert`, `convert-dir`, and `convert-marketplace` write `.cursor-plugin/plugin.json` when that target is set. Cursor conversion is independent of the OpenCode commands. Agents land in `agents/`; hooks land in `hooks/hooks.json` (with Bash-only tool events specialized to `beforeShellExecution` / `afterShellExecution`).
