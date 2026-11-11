#!/usr/bin/env python3
"""
Generate English LP screenshots for MrCodeReview using Playwright and Vite.
Outputs directly to ~/Git/MrkTools/public/assets/mrcodereview/media/
"""
import json
import os
import pathlib
import subprocess
import sys
import time

from playwright.sync_api import sync_playwright

ROOT = pathlib.Path(__file__).resolve().parent.parent  # app/
OUTPUT_DIR = pathlib.Path.home() / "Git/MrkTools/public/assets/mrcodereview/media"

EXE = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
if not os.path.exists(EXE):
    EXE = "/Users/hitoshi/Library/Caches/ms-playwright/chromium_headless_shell-1234/chrome-headless-shell-mac-arm64/chrome-headless-shell"

NOW = 1793000000000
REPO = "/Users/me/Git/shop-api"

FINDINGS = [
    {
        "file": "db.js",
        "line": 4,
        "severity": "high",
        "title": "Direct SQL string concatenation from user input",
        "detail": "The `id` parameter is concatenated directly into the SQL query without parameterization. Attackers can pass inputs like '1 OR 1=1' to return all users. Use a placeholder ($1) instead.",
        "evidence": "pool.query('SELECT * FROM users WHERE id = ' + id)",
        "verified": True,
        "rule": None,
    },
    {
        "file": "config.js",
        "line": 3,
        "severity": "high",
        "title": "Hardcoded secret in source code",
        "detail": "Committing this file will expose secrets in git history. Read this from an environment variable instead.",
        "evidence": "secret: 'demo-not-a-real-secret'",
        "verified": True,
        "rule": None,
    },
    {
        "file": "app.py",
        "line": 12,
        "severity": "medium",
        "title": "Unbounded retry loop",
        "detail": "Retries indefinitely on continuous failures without backoff or maximum retry limit.",
        "evidence": "while True:  # retry forever",
        "verified": False,
        "rule": None,
    },
]

FILES = [
    {"path": "db.js", "bytes": 1180},
    {"path": "config.js", "bytes": 420},
    {"path": "app.py", "bytes": 1721},
]

HANDOFF = (
    "Please fix the following code review findings.\\n"
    "(MrCodeReview / Security / HEAD 6dae70c8)\\n"
    "ID: mrcr:r1\\n\\n"
    "## Fix\\n\\n"
    "1. [high] Direct SQL string concatenation from user input\\n"
    "   Location: db.js:4\\n"
    "   Quote: pool.query('SELECT * FROM users WHERE id = ' + id)\\n"
    "   Scenario: Passing '1 OR 1=1' as id returns all user records\\n\\n"
    "2. [high] Hardcoded secret in source code\\n"
    "   Location: config.js:3\\n\\n"
    "## Not now\\n\\n"
    "3. Unbounded retry loop (app.py:12)"
)

RECIPES = [
    {"id": "security", "title": "Security", "source": "builtin", "kind": "llm", "prompt": "# Security Review\nCheck injection, auth, and secrets."},
    {"id": "bugs", "title": "Latent Bugs", "source": "builtin", "kind": "llm", "prompt": "# Latent Bugs\nFocus on edge cases and logic bugs."},
    {"id": "errors", "title": "Error Handling", "source": "builtin", "kind": "llm", "prompt": "# Error Handling\nFind swallowed errors."},
    {"id": "performance", "title": "Performance", "source": "builtin", "kind": "llm", "prompt": "# Performance\nCheck loops and heavy allocations."},
    {"id": "tests", "title": "Missing Tests", "source": "builtin", "kind": "llm", "prompt": "# Missing Tests\nIdentify untested branches."},
    {"id": "consistency", "title": "Cross-file Inconsistencies", "source": "builtin", "kind": "llm", "prompt": "# Consistency\nCheck interface contract mismatches."},
    {"id": "lint", "title": "Linters", "source": "builtin", "kind": "tool", "prompt": "Run linters without LLM."},
]

def make_demo_data():
    return {
        "providers": [
            {"id": "anthropic-api", "label": "Anthropic API", "note": "Direct communication with Anthropic. No relay server."}
        ],
        "recipes": {"recipes": RECIPES, "warnings": [], "lint": None},
        "snap": {
            "repo": REPO,
            "head": "6dae70c8",
            "diff_hash": "e4ef07e6",
            "diff_bytes": 4821,
            "files": FILES,
            "excluded": [{"path": "package-lock.json", "reason": "Lock file"}],
            "estimated_input_tokens": 1205,
        },
        "hist": [
            {
                "id": "r1",
                "created_at": NOW - 3600000,
                "repo": REPO,
                "recipe_title": "Security",
                "provider_label": "Anthropic API",
                "findings": 3,
                "verified": 2,
                "total_tokens": 1820,
                "failed": False,
                "counts": {"open": 1, "done": 2, "wait": 0, "dismiss": 0},
            },
            {
                "id": "r2",
                "created_at": NOW - 7200000,
                "repo": "/Users/me/Git/blog-engine",
                "recipe_title": "Latent Bugs",
                "provider_label": "Anthropic API",
                "findings": 2,
                "verified": 2,
                "total_tokens": 1450,
                "failed": False,
                "counts": {"open": 0, "done": 2, "wait": 0, "dismiss": 0},
            },
            {
                "id": "r3",
                "created_at": NOW - 86400000,
                "repo": "/Users/me/Git/dotfiles",
                "recipe_title": "Linters",
                "provider_label": "Linters",
                "findings": 0,
                "verified": 0,
                "total_tokens": 0,
                "failed": False,
                "counts": {"open": 0, "done": 0, "wait": 0, "dismiss": 0},
            },
        ],
        "review": {
            "id": "r1",
            "created_at": NOW - 3600000,
            "repo": REPO,
            "recipe": RECIPES[0],
            "provider": {"id": "anthropic-api", "label": "Anthropic API", "note": ""},
            "snapshot": {
                "head": "6dae70c8",
                "diff_hash": "e4ef07e6",
                "diff_bytes": 4821,
                "files": FILES,
                "excluded": [{"path": "package-lock.json", "reason": "Lock file"}],
            },
            "prompt": "Review prompt sent to AI...",
            "model": "claude-sonnet-5-5",
            "response_text": "",
            "raw": "",
            "usage": {
                "input_tokens": 1500,
                "output_tokens": 320,
                "cache_read_tokens": 3200,
                "cache_write_tokens": 410,
            },
            "cost_usd": 0.042,
            "duration_ms": 6400,
            "carries": [None, None, None],
            "fates": [],
            "notes": [],
            "related": None,
            "parse_error": None,
            "error": None,
            "findings": FINDINGS,
            "triage": ["fix", "fix", "wait"],
            "agent_reports": [None, None, None],
            "finding_notes": [
                {"memo": "", "memo_at": None, "done_at": None, "done_head": None, "triage_at": None, "triage_head": None, "triage_source": None}
                for _ in FINDINGS
            ],
        },
        "usage": {
            "since": None,
            "total": {"key": "Total", "reviews": 5, "failed": 0, "unreported": 0, "input": 6800, "output": 1200, "cache_read": 8400, "cache_write": 920},
            "by_recipe": [],
            "by_repo": [],
            "by_model": [],
            "cost_usd": 0.128,
            "cost_reviews": 5,
        },
        "settings": {
            "values": {"language": "en"},
            "meta": [],
            "path": "/mock/settings.json",
            "exists": True,
            "error": None,
            "repo": {"path": None, "keys": [], "error": None},
        },
        "report": {
            "html": f"<div style='font-family: sans-serif; padding: 20px;'><h1>Review Report: {REPO}</h1><p>3 findings recorded.</p></div>",
            "summary": {"reviews": 3, "failed": 0, "findings": 3, "fix": 2, "wait": 1, "dismiss": 0, "new": 0, "done": 0, "input": 1500, "output": 320, "cache_read": 3200, "cache_write": 410, "unreported": 0},
        },
    }

MOCK_SCRIPT = """
window.__DATA = %s;
window.__TAURI_INTERNALS__ = {
  transformCallback: () => 0,
  invoke: async (cmd, args) => {
    const D = window.__DATA;
    switch (cmd) {
      case 'list_providers': return D.providers;
      case 'list_bases': return [{value: 'HEAD', label: 'Uncommitted changes', hint: 'Changes in working tree'}, {value: 'HEAD~1', label: 'Latest commit', hint: 'Last commit and uncommitted'}];
      case 'list_recipes': return args && args.repo ? D.recipes : {recipes: D.recipes.recipes.filter(r => r.kind !== 'tool'), warnings: [], lint: null};
      case 'preview_snapshot': return D.snap;
      case 'list_reviews': return D.hist;
      case 'get_review': return D.review;
      case 'get_usage': return D.usage;
      case 'get_settings': return D.settings;
      case 'get_report': return D.report;
      case 'repos_load': return {repos: [{id:'a',path:'/Users/me/Git/shop-api'},{id:'b',path:'/Users/me/Git/blog-engine'},{id:'c',path:'/Users/me/Git/dotfiles'}], limit: 3, warnings: []};
      case 'repo_status': return (args.paths||[]).map(p => ({path: p, state: 'ok', root: null}));
      case 'repo_data': return (args.paths||[]).map((p, i) => ({path: p, reviews: 3, findings: 9, triaged: 5, memos: 2, open: [24, 7, 13][i] ?? 0}));
      case 'byok_status': return {supported:true,key_set:true,key_last4:'a1B2',key_error:null,model:'claude-sonnet-5-5',max_tokens:8192,config_error:null};
      case 'build_handoff': return '%s';
      case 'write_handoff': return {text: '%s', path: '%s/.mrcodereview/out/handoff.md', rel: '.mrcodereview/out/handoff.md', count: 2, prompt: 'Read .mrcodereview/out/handoff.md and fix the findings listed under "Fix". Do not touch "Not now".'};
      case 'set_triage': {
        window.__TRI = window.__TRI || D.review.triage.slice();
        window.__TRI[args.index] = args.status;
        return {triage: window.__TRI.slice(), notes: D.review.finding_notes};
      }
      default: return null;
    }
  },
};
try {
  localStorage.setItem('mrcr.repo', '/Users/me/Git/shop-api');
  localStorage.setItem('mrcr.base', 'HEAD');
  localStorage.setItem('mrcodereview.language', 'en');
} catch (e) {}
""" % (json.dumps(make_demo_data()), HANDOFF.replace("'", "\\'"), HANDOFF.replace("'", "\\'"), REPO)

def capture_shots(port=1430):
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)
    with sync_playwright() as p:
        browser = p.chromium.launch(executable_path=EXE)
        context = browser.new_context(
            viewport={"width": 1440, "height": 900},
            locale="en-US",
            color_scheme="light",
            device_scale_factor=2,
        )
        page = context.new_page()
        page.add_init_script(MOCK_SCRIPT)
        
        print(f"Connecting to http://localhost:{port}/...")
        page.goto(f"http://localhost:{port}/")
        page.wait_for_selector(".topbar", timeout=10000)
        page.wait_for_timeout(600)

        # 1. hero-en.png: Main review screen with findings
        print("Taking hero-en.png...")
        page.click(".history li >> nth=0 >> .hbody")
        page.wait_for_timeout(400)
        page.screenshot(path=str(OUTPUT_DIR / "hero-en.png"))

        # 2. feature-evidence-en.png: Findings showing verified & mismatch
        print("Taking feature-evidence-en.png...")
        page.screenshot(path=str(OUTPUT_DIR / "feature-evidence-en.png"))

        # 3. feature-sent-en.png: Diff to send panel
        print("Taking feature-sent-en.png...")
        # Click on summary for diff to send
        send_summary = page.locator("summary:has-text('Diff to send')")
        if send_summary.count() == 0:
            send_summary = page.locator(".diff-summary")
        if send_summary.count() > 0:
            send_summary.first.click()
            page.wait_for_timeout(400)
        page.screenshot(path=str(OUTPUT_DIR / "feature-sent-en.png"))

        # Close diff summary if opened
        if send_summary.count() > 0:
            try:
                send_summary.first.click()
            except Exception:
                pass
            page.wait_for_timeout(200)

        # 4. feature-handoff-en.png: Hand off to CLI
        print("Taking feature-handoff-en.png...")
        handoff_btn = page.locator("button:has-text('to CLI')")
        if handoff_btn.count() > 0:
            handoff_btn.first.click()
            page.wait_for_timeout(400)
            # Open preview if present
            preview = page.locator(".handoffprev summary")
            if preview.count() > 0:
                preview.first.click()
                page.wait_for_timeout(300)
                page.evaluate("document.querySelector('.detail')?.scrollTo(0, 99999)")
        page.screenshot(path=str(OUTPUT_DIR / "feature-handoff-en.png"))

        # 5. feature-report-en.png: Report dialog / screen
        print("Taking feature-report-en.png...")
        report_btn = page.locator("button[title*='Report'], button:has-text('Report')")
        if report_btn.count() > 0:
            report_btn.first.click()
            page.wait_for_timeout(500)
        page.screenshot(path=str(OUTPUT_DIR / "feature-report-en.png"))

        browser.close()
        print("All screenshots successfully captured!")

if __name__ == "__main__":
    capture_shots()
