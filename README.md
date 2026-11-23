<p align="center"><img src="art/icon.png" alt="MrCodeReview" width="128"></p>

# MrCodeReview

**Catch the bugs that slip in, before you commit.**  
An open-source AI code review tool that inspects your uncommitted git changes aspect-by-aspect (bugs, errors, security, performance, tests, consistency, Terraform ×5, documents ×3) and verifies citation accuracy to detect hallucinations.

Provides both a visual **Mac desktop app (GUI)** and a command-line **CLI tool (`mrcr`)**, 100% free with no feature restrictions.

[日本語](README.ja.md)

<p align="center">
  <img src="art/hero-en.png" alt="MrCodeReview Desktop App Screenshot" width="900">
</p>

---

## Features

- **GUI or CLI, whatever fits your workflow**: Both licensed under MIT with all features fully unlocked. Review history and triage data are seamlessly shared via a local SQLite database.
- **Direct BYOK (Bring Your Own Key)**: Direct communication with the Anthropic API using your local environment variables or macOS Keychain. No middleman relay servers (100% local and privacy-safe).
- **Isolated Aspect Reviews**: Instead of generic code reviews, each aspect is executed independently for maximum focus and depth, then bundled into a single report.
- **Hallucination Detection**: Strictly verifies quoted lines against actual git diffs, automatically labeling them `Evidence found` or `⚠️ Evidence mismatch`.
- **Coding Agent Handoff**: Automatically outputs findings to `.mrcodereview/out/review.md` (or structured JSON). Hand off directly to coding agents like `Claude Code` or `agy` for automated fix loops.
- **Automatic review after each commit (Git hook)**: Turn it on from the desktop app (Settings → Git Hook) or with `mrcr hook enable`. Only the commit you just made is reviewed, in the background, and the commit is never blocked. A run log shows which commit, which recipes, how many findings and how long it took.
- **Pure Rust & Tauri v2**: Ultra-lightweight and fast. Native Apple Silicon support (Developer ID signed & Apple notarized).

---

## Installation

### 1. Mac Desktop App (GUI)

Download the DMG from the official site or GitHub Releases:

- **[Download from Official Site (mrktools.app)](https://mrktools.app/en/mrcodereview/download/)**
- **[Download from GitHub Releases](https://github.com/MR-TABATA/MrCodeReview/releases)**

> **Requirements**: macOS 13.0 or later (Apple Silicon)

### 2. CLI Tool (`mrcr`)

#### Homebrew (macOS)

```bash
brew install mr-tabata/tap/mrcr
```

#### npx (macOS, no install)

Needs Node.js 18 or later. Pin the version: without one, `npx` runs the latest code on every start.

```bash
npx @mr-tabata/mrcr@1.11.2 --help
```

macOS only for now. The binary is universal (Apple Silicon and Intel); it has been tested on Apple Silicon only.

#### Build with Cargo

```bash
git clone https://github.com/MR-TABATA/MrCodeReview.git
cd MrCodeReview
cargo install --path crates/mrcr
```

---

## Usage

### 1. Configure API Key

No relay servers. Your machine communicates directly with the LLM API:

- **Desktop App**: Open Settings (⌘,) from the top right and enter your Anthropic API key (safely stored in macOS Keychain).
- **CLI**: Export via environment variable:
  ```bash
  export ANTHROPIC_API_KEY="sk-ant-..."
  ```

---

### 2. Desktop App Workflow

1. Launch the app and add your Git repository from the left sidebar.
2. Select review angles (e.g., `Latent Bugs`, `Security`) from the top bar and click **"Run Review"**.
3. Triage the findings into **Fix**, **Wait**, or **Dismiss**.
4. Click **"Send to CLI"** to generate the agent hand-off prompt and Markdown file.

---

### 3. CLI (`mrcr`) Commands

```bash
# Review uncommitted changes for latent bugs (default)
mrcr review

# Run multiple angles (comma-separated)
mrcr review -r bugs,security

# Run all angles (built-in + custom recipes in .mrcodereview/recipes/)
mrcr review --all

# Output to stdout
mrcr review -r bugs --stdout

# Output in JSON format (for CI/CD or scripts)
mrcr review -r bugs -f json --stdout

# Specify comparison base (e.g. changes since HEAD~1)
mrcr review -b HEAD~1

# Review only the last commit (HEAD~1..HEAD), or one given commit (uncommitted changes are not included)
mrcr review --commit
mrcr review --commit <hash>

# List available review angles
mrcr recipes
```

---

### 4. Automatic Review After Each Commit (Git Hook)

Turn it on in the desktop app (**Settings → Git Hook**, with the repository selected) or from the CLI:

```bash
mrcr hook enable -r bugs,errors   # install the hook and turn it ON (recipes are saved)
mrcr hook status                  # ON/OFF, recipes, which mrcr the hook calls, last run
mrcr hook log                     # recent runs: commit, per-recipe findings and time (-n N, --json)
mrcr hook disable                 # remove the hook (recipe settings are kept)
```

- **What it does**: after each `git commit` in that repository, a `post-commit` hook reviews **only that commit** (`mrcr review --commit <hash>`) in the background, one recipe at a time. If one recipe fails, the others still run. Results go into the normal review history, so you can triage them in the desktop app. The commit is **never blocked** (there is no blocking mode yet).
- **Existing hooks are kept**: only a clearly marked block is added to (or removed from) `.git/hooks/post-commit`. Repositories whose `core.hooksPath` points outside `.git` (e.g. husky) are refused instead of being modified.
- **Recipes that need a spec document** (such as `spec`) require a spec file inside the repository (`.md` / `.txt`, relative path): set it in the Git Hook tab or with `mrcr hook enable -r spec --spec docs/spec.md`.
- **Settings are stored in the app data directory** (`hooks.json`, and the run log `hooks.log`), not in the repository, so a cloned repository can never change what runs on your commits.

**Requirements and limits**

- The `mrcr` CLI must be installed (Homebrew, or `npm i -g @mr-tabata/mrcr`). A copy started with `npx` lives in a temporary cache and cannot be used by the hook.
- The hook runs the CLI, which reads the **`ANTHROPIC_API_KEY` environment variable**. A key saved in the desktop app (Keychain) is not visible to it. If the key is missing, the run is recorded as a failure in the run log (the commit is not affected).
- **Every commit calls the LLM API once per selected recipe** (pay-as-you-go). Select only the recipes you need.
- macOS only. Behavior with `git commit --amend` and rebase has not been verified.

---

## CLI Output Example

Example of reviewing a deliberately vulnerable Terraform file (hardcoded DB password, port 5432 open to `0.0.0.0/0`, deletion protection disabled):

```bash
mrcr review -r tf-security,tf-replace -l en
```

![mrcr review output example](art/review-demo.png)

Each finding cites the exact line in the diff, verified against actual changes (`Verified` vs `Unverified evidence`).

---

## Review Angles (Recipes) Catalog

Each recipe focuses on **one specific concern**. Select with `-r <id>` or run all with `--all`.

### Code (6 angles)

| ID | Focus |
|---|---|
| `bugs` | Code that works now but breaks on specific input: off-by-one, null / empty handling, units/signs, condition bugs, state update omissions, async races, unclosed resources, timezone/encoding issues |
| `errors` | Swallowed or untraceable failures: empty `catch`, fallback values on error, unchecked return codes, lost root causes, `unwrap`, missing retry limits |
| `security` | Injections (SQL, command, path, XSS), missing auth/authz, hardcoded secrets, unvalidated input (SSRF, deserialization), crypto misuse |
| `performance` | Code that degrades under scale: algorithmic complexity, N+1 / loop I/O, serial async, redundant copies, memory growth, UI thread blocking |
| `tests` | Untested behavioral changes: missing branch/failure tests, stale assertions, flaky tests depending on time/sleep, fixes without regression tests |
| `consistency` | Inconsistencies with untouched code: stale callers of modified signatures, renamed symbols with leftover references. Inspects related code outside diff |

### Terraform / HCL (5 angles)

| ID | Focus |
|---|---|
| `tf-state` | Resource address to state mapping: `count`/`for_each` conversions, key renames without `moved`, faulty `import`/`removed` |
| `tf-replace` | Unintended recreation / data loss: attributes forcing recreation, misused `lifecycle`, apply-order downtime, deleting stateful resources |
| `tf-repro` | Environmental / temporal instability: loose version constraints, lockfile drift, major bumps, `timestamp()` calls |
| `tf-security` | Overly permissive IAM, open access (`0.0.0.0/0`, public buckets), disabled encryption/logging, unmasked secrets |
| `tf-refs` | Cross-file/cross-environment variable, output, and module link validity |

### Documents (3 angles)

Automatically evaluated when `.md`, `.mdx`, `.txt`, or `.rst` files are in the diff.

| ID | Focus |
|---|---|
| `doc-claims` | Unsubstantiated numbers, claims, or absolutes ("always", "none", "unrelated") |
| `doc-reality` (Experimental) | Unverified factual assertions regarding external commands, package names, versions, pricing, or limits |
| `doc-steps` | Gaps preventing reader action: missing preconditions, unmade decisions treated as final, missing rationale |

### Customization

Add custom recipes by creating Markdown files in `.mrcodereview/recipes/<id>.md`:

- **Same ID**: Overrides built-in recipe.
- **New ID**: Adds a custom review angle.

Built-in prompt templates can be inspected in [`recipes/`](recipes) (Japanese) and [`recipes/en/`](recipes/en) (English).

---

## Coding Agent (Claude Code / agy) Integration

### 1. Simple Workflow
```bash
# 1. Run review (.mrcodereview/out/review.md generated)
mrcr review -r bugs

# 2. Instruct agent to fix
claude "Read .mrcodereview/out/review.md and fix the findings"
```

### 2. Human-in-the-Loop Triage (`mrcr handoff`)
1. Review findings in the desktop app (or CLI) and mark target issues as **Fix**.
2. Let the agent fetch only the triaged items:
   ```bash
   mrcr handoff get             # Fetch all items marked Fix
   mrcr handoff get --json      # Fetch structured JSON
   ```
3. Agent applies changes and reports back:
   ```bash
   mrcr handoff report <review_id> 1,2 --did --text "Fixed null check"
   ```
4. Status and notes update in real-time in the desktop app.

> Claude Code custom skill available in [`skills/mrcr-handoff`](skills/mrcr-handoff).

### 3. Two-way Integration via MCP (Recommended)

With `mrcr mcp`, coding agents like Claude Code, `agy` (Google Antigravity), and VS Code Copilot can directly query and update the local MrCodeReview SQLite database via the standard Model Context Protocol. No manual file management or sync clicks required.

```bash
# Automatically register mrcr mcp in agy / Claude Code config
mrcr mcp setup
```

Without installing `mrcr`, you can use `npx` instead. Pin the version here too:

```bash
npx @mr-tabata/mrcr@1.11.2 mcp setup
```

Or write the entry yourself (for example `.vscode/mcp.json`):

```json
{
  "mcpServers": {
    "mrcodereview": {
      "command": "npx",
      "args": ["-y", "@mr-tabata/mrcr@1.11.2", "mcp"]
    }
  }
}
```

Update the version in this entry yourself when you upgrade. Newer versions upgrade the review database, and an older pinned version refuses to open a newer database ("update this app"). If the agent is launched from the Dock or Finder, its `PATH` may not contain `npx`; in that case, give the full path to `npx` as `command`.

Once registered, simply ask your agent in your everyday terminal session:
> "Check MrCodeReview findings for MrDiff and fix them."

The agent fetches the triaged items, applies the fixes, and records the resolution in the DB. The desktop app updates to "Resolved" in real-time.

---

## License

MIT License © 2026 Hitoshi TABATA
