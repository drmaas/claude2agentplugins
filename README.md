# c2ap — Claude Plugin → Agent Plugins Converter

Convert [Claude Code](https://docs.anthropic.com/en/docs/claude-code) plugins to [Agent Plugins](https://agent-plugins.org) v1.0.0 format.

## Installation

```bash
cargo install c2ap
```

## Usage

### Convert a single plugin

```bash
c2ap convert ./my-claude-plugin -o ./output
```

### Convert a directory of plugins

Each subdirectory containing `.claude-plugin/plugin.json` is converted.

```bash
c2ap convert-dir ./plugins-dir -o ./output
```

### Convert a marketplace repo

```bash
c2ap convert-marketplace --repo anthropics/claude-plugins-official -o ./output
```

## Flags

| Flag | Description | Default |
|------|-------------|---------|
| `-o, --output` | Output directory | `./output` |
| `--extension-namespace` | Extension namespace for Claude-specific data | `com.claude.code` |
| `--strict` | Fail on any warning | `false` |
| `-n, --dry-run` | Show what would be done without writing | `false` |
| `-v, --verbose` | Verbose output | `false` |
| `-q, --quiet` | Suppress output except errors | `false` |

## Conversion Mappings

### Directly mapped

| Claude Plugin | Agent Plugins |
|---|---|
| `.claude-plugin/plugin.json` → | `plugin.json` (with `$schema`) |
| `skills/` → | `skills/` (validates SKILL.md frontmatter) |
| `.mcp.json` → | `mcp.json` (adds `type: "stdio"`, transforms env vars) |

### Moved to extensions

Claude-specific components that have no Agent Plugins equivalent are preserved under `extensions["com.claude.code"]`:

- `commands/`, `agents/`, `hooks/`, `scripts/`
- `themes/`, `monitors/`, `workflows/`, `output-styles/`
- `.lsp.json`
- Manifest fields: `displayName`, `metadata`, `dependencies`, `userConfig`, `channels`, `experimental`

### Environment variable transformation

| Claude | Agent Plugins |
|---|---|
| `${CLAUDE_PLUGIN_ROOT}` | `${PLUGIN_ROOT}` |
| `${CLAUDE_PLUGIN_DATA}` | `${PLUGIN_DATA}` |
| `${CLAUDE_PROJECT_DIR}` | *(warning — no equivalent)* |

## Development

```bash
cargo test
cargo build --release
```
