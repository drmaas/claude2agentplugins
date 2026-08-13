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
