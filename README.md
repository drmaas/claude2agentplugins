# c2ap — Claude Plugin Converter

Convert [Claude Code](https://code.claude.com/docs/en/plugins) plugins to [Agent Plugins](https://agent-plugins.org) v1.0.0, a [Cursor plugin](https://cursor.com/docs/plugins), or an [OpenCode v2](https://opencode.ai/v2/docs/plugins/) plugin package. The default target is Agent Plugins. Pass `--target cursor` for Cursor's `.cursor-plugin/plugin.json` layout, or use `convert-opencode` for an OpenCode v2 plugin package.

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
| `--target` | `agent-plugins` (default) or `cursor` | `agent-plugins` |
| `-o, --output` | Output directory | `./output` |
| `--extension-namespace` | Extension namespace for Claude-specific data | `com.claude.code` |
| `--strict` | Fail on any warning | `false` |
| `--convert-commands` | Convert `commands/` to skills (Agent Plugins: portable skills; Cursor: `disable-model-invocation: true`). OpenCode always registers commands and ignores this flag | `false` |
| `--force` | Overwrite non-empty output directories | `false` |
| `--json` | Emit a machine-readable JSON summary | `false` |
| `-n, --dry-run` | Show what would be done without writing | `false` |
| `-v, --verbose` | Verbose output | `false` |
| `-q, --quiet` | Suppress output except errors | `false` |

## OpenCode v2

`convert-opencode` writes an OpenCode v2 plugin package. Load it by placing the directory in `.opencode/plugins/` (discovered automatically) or by adding its path to `plugins` in `opencode.json(c)`. A `plugins/` directory next to a project-root `opencode.json(c)` is not discovered on its own.

```bash
c2ap convert-opencode ./my-claude-plugin -o ./opencode-plugin
c2ap convert-opencode-dir ./plugins-dir -o ./output
c2ap convert-opencode-marketplace --repo ./local-marketplace -o ./output
c2ap validate-opencode ./opencode-plugin
```

The package follows the [v2 plugin guide](https://opencode.ai/v2/docs/build/plugins/):

```json
{
  "name": "opencode-sample-plugin",
  "version": "1.2.3",
  "type": "module",
  "exports": { ".": "./src/index.ts" },
  "dependencies": { "@opencode/plugin": "latest" }
}
```

`src/index.ts` default-exports `Plugin.define({ id, setup })`. `setup` registers converted components with the v2 transform APIs.

### Mapping

| Claude plugin | OpenCode v2 |
|---|---|
| Plugin package | `package.json` (`type: "module"`, `exports["."]`, dependency `@opencode/plugin`) and `src/index.ts` |
| `skills/*/SKILL.md`, root `SKILL.md` | `ctx.skill.transform` → `editor.add({ id, name, description, location, content })`. The skill file is also written at `skills/<name>/SKILL.md` and `location` points at it |
| `commands/*.md` | `ctx.command.transform` → `editor.add({ name, description, execute })`. `execute` calls `ctx.session.prompt` with the command body. `$ARGUMENTS` is replaced with `prompt.text` |
| `agents/*.md` | `ctx.agent.transform` → `editor.update(id, ...)`. Sets `description`, `mode: "subagent"`, `system` (the prompt), `permissions`, and `color` / `steps` when they match [Agent.Info](https://opencode.ai/v2/docs/build/plugins/). v2 has no `editor.add` for agents; `update` supplies the fields on that id |
| `hooks/hooks.json` command hooks when a faithful mapping exists | Registered in `setup` via OpenCode v2 hook APIs (see table below). Commands run through `child_process.spawnSync` with a best-effort Claude-compatible stdin JSON payload. `${CLAUDE_PLUGIN_ROOT}` becomes the package root at runtime; `scripts/` is copied when referenced |
| `.mcp.json` stdio | `ctx.mcp.transform` → `editor.set(name, { type: "local", command, environment, cwd })`. `command` is a string array. `${CLAUDE_PLUGIN_ROOT}` becomes `path.join(root, ...)` inside the package |
| `.mcp.json` `http` and `sse` | `editor.set(name, { type: "remote", url, headers })`. SSE is warned because v2 has one remote transport |
| `README.md`, `LICENSE`, `CHANGELOG.md` | Copied to the package root |

OpenCode v2 agent permissions are `{ action, resource, effect }` rules. A Claude `tools` allowlist becomes a deny-all rule followed by `allow` rules for the mapped actions (`read`, `edit`, `bash`, and the other built-in tool names). An omitted tools list becomes `{ action: "*", resource: "*", effect: "allow" }`.

### Hook mapping (OpenCode v2)

| Claude hook event | OpenCode v2 registration | Notes |
|---|---|---|
| `PreToolUse` (Bash / Shell matcher) | `ctx.shell.hook("create.before")` and `ctx.permission.hook("evaluate")` | Deny / exit 2 rewrites the shell command to `false` (best-effort block) and sets permission `effect` |
| `PreToolUse` (other / mixed tools) | `ctx.tool.hook("execute.before")` and `ctx.permission.hook("evaluate")`; Bash portion also gets `ctx.shell.hook` | `updatedInput` is applied when present; deny / exit 2 throws |
| `PostToolUse` | `ctx.tool.hook("execute.after")` (status `completed`) | Applies `updatedToolOutput` when present |
| `PostToolUseFailure` | `ctx.tool.hook("execute.after")` (status `error`) | Observational |
| `UserPromptSubmit` | `ctx.session.hook("prompt")` | Block clears / annotates prompt text (no typed rejection API) |
| `PermissionRequest` | `ctx.permission.hook("evaluate")` | Maps `permissionDecision` / `decision.behavior` to `effect` |
| `PreCompact` | `ctx.session.hook("compaction")` | Side effects only |
| `Notification`, `SessionStart`, `SessionEnd`, `Stop`, `StopFailure`, `SubagentStart`, `SubagentStop`, `Setup`, and other Claude-only events | *sidecar only* | No faithful OpenCode v2 hook; warned and preserved under `extensions/` |
| Hook types other than `command` (`prompt`, `http`, `mcp_tool`, `agent`) | *sidecar only* | OpenCode hooks are TypeScript callbacks; those Claude handler kinds are not translated |

### Sidecar

Claude data that v2 plugins do not load is copied to `extensions/<namespace>/` (default `extensions/com.claude.code/`) and reported as a warning:

- Original `hooks/` (always kept, including when command hooks were also mapped onto `ctx.*.hook`)
- `workflows/`, `monitors/`, `output-styles/`, `themes/`, `evals/`, `bin/`
- `scripts/` when not copied to the package root for mapped hooks
- `.lsp.json`, `settings.json`, and the original `.mcp.json`
- Manifest `userConfig`, `dependencies`, `channels`, and `defaultEnabled`

`--convert-commands` applies to Agent Plugins and Cursor. OpenCode conversion always registers `commands/` through `ctx.command.transform` and ignores this flag.

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

## Cursor target

```bash
c2ap convert ./my-claude-plugin -o ./output --target cursor
c2ap convert-dir ./plugins-dir -o ./output --target cursor
c2ap convert-marketplace --repo anthropics/claude-plugins-official -o ./output --target cursor
```

`convert` writes one plugin. `convert-dir` and `convert-marketplace` also write `.cursor-plugin/marketplace.json` so the output directory can be imported as a Cursor multi-plugin repository.

Install a converted plugin locally by copying it to `~/.cursor/plugins/local/<name>/` (the folder must contain `.cursor-plugin/plugin.json`), then reload Cursor and confirm the components in Customize. Cursor's documented layout is the [Plugins reference](https://cursor.com/docs/reference/plugins):

```text
my-plugin/
├── .cursor-plugin/plugin.json
├── rules/
├── skills/<name>/SKILL.md
├── agents/
├── commands/
├── hooks/hooks.json
├── mcp.json
├── scripts/
└── README.md
```

Validate Cursor output with `c2ap validate ./output` (auto-detected when `.cursor-plugin/plugin.json` is present and a root `plugin.json` is not) or `c2ap validate --target cursor ./output`.

## Cursor conversion mappings

Cursor output follows the current Cursor plugin docs: a `.cursor-plugin/plugin.json` manifest (only `name` is required), folder discovery for skills, rules, agents, commands, hooks, and `mcp.json`, and `${CURSOR_PLUGIN_ROOT}` in MCP config. Skills use the Agent Skills `SKILL.md` shape Cursor documents (`name` matches the parent folder). Rules are `.mdc` files with `description`, `alwaysApply`, and `globs`.

### Directly mapped

| Claude plugin | Cursor plugin |
|---|---|
| `.claude-plugin/plugin.json` `name`, `version`, `description`, `author.name`, `author.email`, `homepage`, `repository`, `license`, `keywords` | `.cursor-plugin/plugin.json` |
| `userConfig` | `variables` JSON Schema. `${user_config.KEY}` becomes `${KEY}`. Set values in the dashboard (Plugins → Configure); they are not written into the plugin |
| `skills/*/SKILL.md`, custom skill paths, root `SKILL.md` | `skills/<name>/SKILL.md` plus `scripts/`, `references/`, `assets/`, and other skill files. `name` is normalized to match the folder |
| `.mcp.json` stdio servers | `mcp.json` entries with `command` / `args` / `env` / `cwd`. `${CLAUDE_PLUGIN_ROOT}` becomes `${CURSOR_PLUGIN_ROOT}` |
| `.mcp.json` `http` / `sse` servers | `mcp.json` entries with `url` and `headers` (Cursor infers transport from the URL) |
| `agents/*.md` | `agents/<name>.md` with `name`, `description`, and the prompt body |
| `commands/*.md` | `commands/<name>.md` with `name` and `description` |
| `commands/*.md` with `--convert-commands` | `skills/<name>/SKILL.md` with `disable-model-invocation: true` (Cursor's explicit slash-command style) and no `commands/` copy |
| `hooks/hooks.json` events Cursor documents (`PreToolUse`, `PostToolUse`, `PostToolUseFailure`, `UserPromptSubmit`, `Stop`, `SubagentStart`, `SubagentStop`, `SessionStart`, `SessionEnd`, `PreCompact`) | `hooks/hooks.json` (`version: 1`). Nested Claude groups are flattened. `Bash` → `Shell`, `Edit` → `Write`. Bash-only `PreToolUse` / `PostToolUse` specialize to `beforeShellExecution` / `afterShellExecution`. `${CLAUDE_PLUGIN_ROOT}/...` becomes `./...` |
| `rules/`, `.claude/rules/`, manifest `rules`, `CLAUDE.md` | `rules/*.mdc`. Claude `paths` becomes Cursor `globs`. A pathless rule or `CLAUDE.md` is `alwaysApply: true` |
| `scripts/` | `scripts/` (Cursor's hook and utility script directory) |
| `bin/` | `bin/` so `${CURSOR_PLUGIN_ROOT}/bin/...` commands keep working |
| `LICENSE`, `README.md`, `CHANGELOG.md` | same paths at the plugin root |

### Warnings and sidecar

Claude-only data is preserved under `<extension-namespace>/` (default `com.claude.code/`) and called out with a warning. That directory is not a Cursor component, so Cursor ignores it.

| Claude feature | What c2ap does |
|---|---|
| `displayName`, `author.url`, manifest `metadata`, `dependencies`, `channels`, `defaultEnabled`, raw `userConfig` | `com.claude.code/manifest-extras.json` |
| Original `.claude-plugin/plugin.json`, `.mcp.json`, and `hooks/hooks.json` | copied into the sidecar |
| Agent frontmatter other than `name` and `description` (`model`, `tools`, `effort`, `maxTurns`, …) | dropped from the Cursor agent file; original fields in `component-extras.json` |
| Command frontmatter other than `name` and `description` (`argument-hint`, `allowed-tools`, …) | dropped from the Cursor command; original fields in `component-extras.json` |
| Hook events with no Cursor equivalent (`Notification`, `PermissionRequest`, `Setup`, …) | omitted from `hooks/hooks.json`; original file kept in the sidecar |
| Hook types other than `command` and `prompt` (`http`, `mcp_tool`, `agent`) | omitted; original file kept in the sidecar |
| `ws` MCP servers, `headersHelper`, servers with no command | omitted from `mcp.json`; original `.mcp.json` kept in the sidecar |
| `${CLAUDE_PLUGIN_DATA}`, `${CLAUDE_PROJECT_DIR}` | left unchanged, with a warning. Cursor expands `${CURSOR_PLUGIN_ROOT}` and `${CLAUDE_PLUGIN_ROOT}` in `mcp.json`, and aliases `CLAUDE_PROJECT_DIR` for hooks only |
| `output-styles/`, `themes/`, `monitors/`, `workflows/`, `.lsp.json`, `settings.json` | copied into the sidecar. LSP, output styles, themes, monitors, and workflows have no Cursor plugin equivalent |
| `bin/` on `PATH` | files are copied, with a warning that Cursor does not prepend `bin/` to the shell `PATH` |
| Skill `globs` | written as Cursor `paths` |

Claude Code does not officially ship rules inside plugins. When `rules/`, `.claude/rules/`, a manifest `rules` path, or `CLAUDE.md` is present, c2ap still maps those files because Cursor plugins do load rules.

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
