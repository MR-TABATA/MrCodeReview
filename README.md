# MrCodeReview CLI (`mrcr`)

**Catch AI-written bugs before you commit.**  
An open-source CLI tool that reviews your uncommitted git changes aspect-by-aspect (bugs, errors, security, performance, tests, consistency) and verifies quotation accuracy to detect hallucinations.

[日本語](README.ja.md)

---

## Features

- **Direct BYOK (Bring Your Own Key)**: Direct calls to Anthropic API using `ANTHROPIC_API_KEY`. No intermediate proxy servers.
- **Isolated Aspect Reviews**: Instead of generic code reviews, each aspect is executed independently for maximum focus and depth, then bundled into a single report.
- **Hallucination Detection**: Strictly verifies cited lines against actual git diffs, distinguishing between `Verified` and `⚠️ Unverified evidence`.
- **Coding Agent Handoff**: Automatically outputs review findings to `.mrcodereview/out/review.md` (or JSON). Read it directly with agents like `agy` or `Claude Code` for automated fix loops.
- **Cross-Platform**: Pure Rust. Runs on macOS and Linux (CI/CD environments like GitHub Actions).
- **Shared History with Desktop App**: On macOS, reviews are recorded to the local SQLite database shared with the desktop GUI app (MrkCodeReview).

---

## Installation

```bash
cargo install --path crates/mrcr
```

---

## Usage

```bash
export ANTHROPIC_API_KEY="sk-ant-..."

# Review uncommitted changes for bugs (default)
mrcr review

# Multiple aspects (comma-separated)
mrcr review -r bugs,security

# Run all built-in aspects
mrcr review --all

# Output to stdout
mrcr review -r bugs --stdout

# Output as JSON
mrcr review -r bugs -f json --stdout

# Specify diff base
mrcr review -b HEAD~1
```

### List available recipes

```bash
mrcr recipes
```

---

## License

MIT License
