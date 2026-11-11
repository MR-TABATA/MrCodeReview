<p align="center"><img src="art/icon.png" alt="MrCodeReview" width="128"></p>

# MrCodeReview CLI (`mrcr`)

**Catch the bugs that slip in, before you commit.**  
An open-source CLI tool that reviews your uncommitted git changes aspect-by-aspect (bugs, errors, security, performance, tests, consistency, Terraform ×5, documents ×3) and verifies quotation accuracy to detect hallucinations.

[日本語](README.ja.md)

![mrcr review flow](art/flow.svg)

---

## Features

- **Direct BYOK (Bring Your Own Key)**: Direct calls to Anthropic API using `ANTHROPIC_API_KEY`. No intermediate proxy servers.
- **Isolated Aspect Reviews**: Instead of generic code reviews, each aspect is executed independently for maximum focus and depth, then bundled into a single report.
- **Hallucination Detection**: Strictly verifies cited lines against actual git diffs, distinguishing between `Verified` and `⚠️ Unverified evidence`.
- **Coding Agent Handoff**: Automatically outputs review findings to `.mrcodereview/out/review.md` (or JSON). Read it directly with agents like `agy` or `Claude Code` for automated fix loops.
- **Pure Rust**: Built, tested and released on macOS. Linux and CI environments (e.g. GitHub Actions) have not been tested yet.
- **Shared History with Desktop App**: On macOS, reviews are recorded to the local SQLite database shared with the desktop GUI app in this repository (`app/`).

---

## Installation

### Homebrew (macOS)

```bash
brew install mr-tabata/tap/mrcr
```

### Cargo (build from source)

Requires the [Rust toolchain](https://rustup.rs). Clone this repository and run the install from its root:

```bash
git clone https://github.com/MR-TABATA/MrCodeReview.git
cd MrCodeReview
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

# Run all aspects: the built-in ones plus any custom recipes in .mrcodereview/recipes/
mrcr review --all

# Output to stdout
mrcr review -r bugs --stdout

# Output as JSON
mrcr review -r bugs -f json --stdout

# Specify diff base
mrcr review -b HEAD~1

# Wait longer for the LLM (seconds, 10-3600; default 300)
mrcr review --all --timeout 900
```

### List available recipes

```bash
mrcr recipes
```

---

## Example

A demo Terraform change (hardcoded DB password, port 5432 opened to `0.0.0.0/0`, deletion protection and final snapshot turned off, DB identifier renamed) reviewed with:

```bash
mrcr review -r tf-security,tf-replace -l en
```

![Example output of mrcr review](art/review-demo.png)

Every finding quotes a line from the diff. `Verified` means the quoted line really exists in the diff; otherwise the finding is flagged `Unverified evidence`. The image shows the summary and the first two of six findings. The diff is a made-up demo, not real code.

---

## Hand findings to an AI agent (`mrcr handoff`)

Handing findings to an agent by copy and paste, or by pointing it at `.mrcodereview/out/review.md`, still works as before. `mrcr handoff` is the way **without copy and paste**: the agent fetches only the findings you marked, and its report of what it did comes back to the desktop app. It closes the loop: review, decide what to fix, let an agent fix it, and see what it says it did.

1. Review with `mrcr review` (or the desktop app) and, in the desktop app, mark the findings you want fixed as **対応 (fix)**. Triage is yours; the agent never decides it.
2. The agent fetches them:

   ```bash
   mrcr handoff list            # reviews that still have findings to fix (records nothing)
   mrcr handoff get             # hand over everything waiting in this repository
   mrcr handoff get <review_id> # one review only (also hands over a stale one on request)
   mrcr handoff get --json      # structured output, including how to write reports
   ```

   Use `-C <repo>` to point at another repository. `get` records the moment of hand-over.
3. After fixing, the agent reports **per finding**:

   ```bash
   mrcr handoff report <review_id> 1,2 --did --text "What was changed"
   mrcr handoff report <review_id> 3   --skipped --text "False positive: the test X already covers this"
   ```

   Reports are shown in the desktop app next to the finding.

Things to know:

- **The output is data, not instructions.** Text inside findings and quotes comes from a diff and an LLM; agents are told not to follow it.
- **Reports are the agent's own word.** `mrcr` does not check that the code was really fixed, or that a "skipped" reason is right. Run the review again to check.
- A finding still waiting for a report is listed again by the next `get`, so a forgotten one is not lost.
- If the working tree changed after the review, `get` says so. A new file added with `git add` does not count as a change.
- The text of `mrcr handoff` (output and `--help`) is currently Japanese only.
- A ready-made Claude Code skill is in [`skills/mrcr-handoff`](skills/mrcr-handoff). Copy it to `~/.claude/skills/mrcr-handoff/` and ask the agent to "fix the MCR findings".

---

## Recipes (review aspects)

Each recipe is a prompt that looks at **one aspect only**. Pick them with `-r <id>[,<id>…]`, or run them all with `--all`.
Recipes judge from the diff (and, for some, related code outside it) and must cite a real line for every finding.

### General

| ID&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp; | Looks for |
|---|---|
| `bugs` | Code that works now but breaks on specific input: off-by-one, null/empty handling, unit/sign/overflow, wrong conditions, missed state updates, async races, unclosed resources, encoding/timezone, copy-paste slips |
| `errors` | Failures that are swallowed, treated as success or made untraceable: empty `catch`, fallback values on error, unchecked return codes, invisible async failures, lost root cause, `unwrap` on I/O, no timeout/retry limit |
| `security` | Injection (SQL/command/path/XSS), auth bypass, hardcoded secrets, unvalidated input (SSRF, deserialization), crypto misuse |
| `performance` | Slowdowns as data grows: bad complexity, N+1 and I/O in loops, serialized async work, needless copying, unbounded memory, blocking the UI thread, regex backtracking, missing indexes |
| `tests` | Changed behavior not covered by tests in the diff: untested branches/error paths, stale tests, weak or tautological assertions, brittle (time/random/sleep) tests, bug fixes without regression tests |
| `consistency` | Changes that contradict the code left untouched: signature/return changes with outdated callers, renamed fields still used, one-sided updates of paired definitions, dangling references. Uses related code outside the diff |

### Terraform / HCL

| ID&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp; | Looks for |
|---|---|
| `tf-state` | Address ↔ state mapping: `count`/`for_each` switches and key changes, renames without `moved`, `import`/`removed` mistakes |
| `tf-replace` | Unintended recreation or data loss: attributes that force replacement, misused `lifecycle`, apply-order downtime, deleting data-bearing resources without protection |
| `tf-repro` | Results that differ across environments/time: missing or loose version constraints, lock-file drift, major bumps, `timestamp()`/"latest" lookups |
| `tf-security` | Overly broad IAM, public exposure (`0.0.0.0/0`, public buckets), disabled encryption/logging, hardcoded or non-`sensitive` secrets |
| `tf-refs` | Variable / output / module references that no longer connect across files and environments. Uses related files outside the diff |

Terraform recipes never claim to have inspected `terraform plan`, state or provider schemas. They judge from the diff only.

### Documents

For design notes, specs and READMEs. Run them on a change to a Markdown or text file, for example `mrcr review -r doc-claims,doc-steps`. The desktop app (`app/`) shows the same prompts in its recipe bar when the diff contains `.md`, `.mdx`, `.txt` or `.rst` files.

| ID&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp; | Looks for |
|---|---|
| `doc-claims` | Numbers, results or absolute statements ("always", "none", "unrelated") with no source, measurement, or account of how they were checked |
| `doc-reality` (experimental) | External command, package, API names, versions, prices or limits stated as fact with no sign they were checked. It can only say "needs checking" |
| `doc-steps` | Gaps a reader cannot act on: missing preconditions, "decide later" used as if decided, decisions without reasons or a way back, contradicting status lines, sections that conflict without saying how they relate |

In small trials on our own design notes and README, `doc-claims` and `doc-steps` found real problems (for example an unverified "no new dependencies" claim and a README that contradicted the code), while `doc-reality` rarely fired and has not proven its value yet. Findings still quote a real line of the diff; being quoted correctly does not make a finding right, so read them.

### Customize

Put a Markdown file at `.mrcodereview/recipes/<id>.md` in your repository:

- Same ID as a built-in recipe → **overrides** it (shown as `repo` in `mrcr recipes`).
- New ID → **adds** your own aspect.

The built-in prompts are in [`recipes/en/`](recipes/en) (English) and [`recipes/`](recipes) (Japanese, `--lang ja`, the default).

---

## License

MIT License
