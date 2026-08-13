# c2ap — Claude Plugin → Agent Plugins Converter

Convert [Claude Code](https://docs.anthropic.com/en/docs/claude-code) plugins to [Agent Plugins](https://agent-plugins.org) v1.0.0 format.

## Installation

These installation methods build the Rust CLI locally, so they require a Rust toolchain and Cargo. Install them with [rustup](https://rustup.rs/) if needed.

### From crates.io

The recommended installation for Rust users is through [crates.io](https://crates.io/crates/c2ap):

```bash
cargo install c2ap
```

This command is available once a release has been published to crates.io.

### Directly from GitHub

To install the latest source from the repository without waiting for a crates.io release:

```bash
cargo install --git https://github.com/drmaas/claude2agentplugins c2ap
```

To install a specific tagged release:

```bash
cargo install --git https://github.com/drmaas/claude2agentplugins --tag v0.1.0 c2ap
```

### From a local checkout

For local development or a modified checkout:

```bash
git clone https://github.com/drmaas/claude2agentplugins.git
cd claude2agentplugins
cargo install --path .
```

All three methods install the `c2ap` executable into Cargo's bin directory, normally `~/.cargo/bin`. Ensure that directory is on your `PATH`.

## Usage

### Convert a single plugin

```bash
c2ap convert ./my-claude-plugin -o ./output
```

Manifest-less plugins (a directory with `skills/`, a root `SKILL.md`, or auto-discovered components) are detected automatically; a manifest is synthesized from the directory name with a warning.

### Convert a directory of plugins

Each subdirectory containing `.claude-plugin/plugin.json`, `skills/`, or a root `SKILL.md` is converted.

```bash
c2ap convert-dir ./plugins-dir -o ./output
```

### Convert a marketplace repo

Accepts a git URL, an `owner/repo` GitHub shorthand, or a local directory path.

```bash
c2ap convert-marketplace --repo anthropics/claude-plugins-official -o ./output
c2ap convert-marketplace --repo ./local-marketplace -o ./output
```

Both the modern `{name, owner, plugins: [...]}` marketplace format and the legacy bare-array format are supported, including relative-path sources (`"./plugins/x"`), `metadata.pluginRoot`, and `github` / `url` / `git-subdir` / `npm` / `archive` sources.

### Validate an existing Agent Plugins plugin

Check a plugin directory (converted or hand-written) against the Agent Plugins v1.0.0 rules without fetching schemas:

```bash
c2ap validate ./output
```

### Scaffold a new Agent Plugins plugin

```bash
c2ap init my-plugin -o ./plugins --description "Does something useful"
```

## Flags

| Flag | Description | Default |
|------|-------------|---------|
| `-o, --output` | Output directory | `./output` |
| `--extension-namespace` | Extension namespace for Claude-specific data | `com.claude.code` |
| `--strict` | Fail on any warning | `false` |
| `--convert-commands` | Also convert `commands/` (flat Markdown skills) to portable skills | `false` |
| `--force` | Overwrite non-empty output directories | `false` |
| `--json` | Emit a machine-readable JSON summary | `false` |
| `-n, --dry-run` | Show what would be done without writing | `false` |
| `-v, --verbose` | Verbose output | `false` |
| `-q, --quiet` | Suppress output except errors | `false` |

## Conversion Mappings

### Directly mapped (portable)

| Claude Plugin | Agent Plugins |
|---|---|
| `.claude-plugin/plugin.json` → | `plugin.json` (with `$schema`) |
| `skills/` → | `skills/` (validates and normalizes SKILL.md frontmatter, copies `scripts/`, `references/`, `assets/`, and any other skill files) |
| `.mcp.json` stdio servers → | `mcp.json` (adds `type: "stdio"`, rewrites commands, transforms env vars) |
| `.mcp.json` http / sse servers → | `mcp.json` streamable-http / sse entries |
| root `SKILL.md` → | single skill (Claude single-skill plugin layout) |
| `commands/*.md` (with `--convert-commands`) → | `skills/<name>/SKILL.md` |

### Moved to extensions

Claude-specific components that have no Agent Plugins equivalent are preserved under `extensions["com.claude.code"]` as files (in a top-level namespace directory) and/or manifest data:

- `commands/`, `agents/`, `hooks/`, `scripts/`, `bin/`
- `themes/`, `monitors/`, `workflows/`, `output-styles/`
- `.lsp.json`, `settings.json`, and the original `.mcp.json`
- Custom component paths declared in the manifest (`"commands": "./custom/..."`, `"hooks": [...]`, etc.)
- Manifest fields: `displayName`, `metadata`, `dependencies`, `userConfig`, `channels`, `experimental`

`LICENSE`, `README.md`, and `CHANGELOG.md` are copied to the output root.

### Environment variable transformation

| Claude | Agent Plugins |
|---|---|
| `${CLAUDE_PLUGIN_ROOT}` in command → | `./`-relative path (Agent Plugins does not expand placeholders in `command`) |
| `${CLAUDE_PLUGIN_ROOT}` in args/env/cwd → | `${PLUGIN_ROOT}` |
| `${CLAUDE_PLUGIN_DATA}` → | `${PLUGIN_DATA}` |
| `${CLAUDE_PROJECT_DIR}` | *(warning — no equivalent)* |
| `${user_config.*}` | *(warning — no equivalent)* |
| `env` keys named `PLUGIN_ROOT`/`PLUGIN_DATA` | *(warning — reserved by Agent Plugins)* |
| `ws` MCP servers | *(warning — no Agent Plugins equivalent; original `.mcp.json` preserved in extensions)* |

### Skill frontmatter normalization

- `name` is normalized to Agent Skills naming rules (lowercase, hyphens; renamed with a warning when needed)
- `allowed-tools` is written as the Agent Skills space-separated string
- `metadata` values are stringified (with a warning when a value is not a string)

## Development

```bash
cargo test
cargo build --release
```

## Publishing

The crate is published to crates.io from versioned releases. Package versions on crates.io are immutable, so increment the version in `Cargo.toml` for every new release.

Before the first publication, inspect the package and run the publish checks without uploading:

```bash
cargo package --list
cargo publish --dry-run --locked
```

For the first release, authenticate locally and publish manually:

```bash
cargo login
cargo publish --locked
```

Do not share or commit the crates.io API token. Create one from the [crates.io API token settings](https://crates.io/settings/tokens), then add it as the `CARGO_REGISTRY_TOKEN` repository secret under **GitHub → Settings → Secrets and variables → Actions**. Pushing a tag matching the package version, such as `v0.1.0`, then publishes new versions automatically and creates the corresponding GitHub release. The workflow skips a version that was already published manually.
