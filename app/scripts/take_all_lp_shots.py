#!/usr/bin/env python3
"""
Generate LP & README screenshots for MrCodeReview (ja and en):
1. hero: Review detail screen (findings, diff, continuity banner, triage)
2. runlog-feed: Initial screen with execution log timeline & inline 2-level expansion

Outputs:
- MrCodeReview/art/hero.png, hero-en.png (2880x2200)
- MrCodeReview/art/runlog-feed.png, runlog-feed-en.png (2880x2200)
- MrkTools/public/assets/mrcodereview/media/hero-v2.png, hero-v2-en.png (2400x1500)
- MrkTools/public/assets/mrcodereview/media/feature-runlog.png, feature-runlog-en.png (2400x1500)
- MrkTools/local/hero-shots/mrcodereview/ja/hero.png, en/hero.png
"""
import copy
import datetime
import importlib.util
import json
import os
import pathlib
import socket
import subprocess
import sys
import time
import urllib.request
from playwright.sync_api import sync_playwright

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent  # MrCodeReview/
APP = ROOT / "app"
MRKTOOLS = pathlib.Path.home() / "Git/MrkTools"

EXE = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
if not os.path.exists(EXE):
    EXE = "/Users/hitoshi/Library/Caches/ms-playwright/chromium_headless_shell-1234/chrome-headless-shell-mac-arm64/chrome-headless-shell"

spec = importlib.util.spec_from_file_location("lp_en", APP / "scripts" / "take_lp_shots_en.py")
lp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lp)

JA_FINDINGS = [
    ("ユーザー入力を SQL に直接つないでいる", "id をそのまま SQL に連結しているため、`1 OR 1=1` のような入力で、全ユーザーの行が返ります。プレースホルダ（$1）に戻してください。"),
    ("秘密の値がソースに直書きされている", "コミットすると、履歴に残ります。環境変数から読む形にしてください。"),
    ("リトライの上限が無い", "失敗が続くと、無限に再試行します。"),
]
JA_RECIPES = ["セキュリティ", "潜在バグ", "例外処理", "パフォーマンス", "テスト不足", "ファイル間の不整合"]

NOW = 1795500000000  # 約 2026-11-25
JST = datetime.timezone(datetime.timedelta(hours=9))


def ms(d, h, m, s):
    return int(datetime.datetime(2026, 11, d, h, m, s, tzinfo=JST).timestamp() * 1000)


def make_hook_status(lang):
    ja = lang == "ja"
    return {
        "repo": "/Users/me/Git/shop-api",
        "enabled": True,
        "recipes": ["bugs", "security"],
        "spec": None,
        "installed": True,
        "cli": "/opt/homebrew/bin/mrcr",
        "hook_cli": "/opt/homebrew/bin/mrcr",
        "blocked": None,
        "target_missing": False,
        "last_run": None,
        "recent": [
            {
                "run": "run-hook-1",
                "started_at_ms": ms(25, 12, 10, 0),
                "commit": "a4f82bc",
                "subject": "SQL クエリのプレースホルダ修正" if ja else "Fix SQL query placeholders",
                "recipes": ["bugs", "security"],
                "steps": [
                    {"recipe": "bugs", "index": 1, "total": 2, "ok": True, "findings": 0, "ms": 2800, "error": None},
                    {"recipe": "security", "index": 2, "total": 2, "ok": True, "findings": 1, "ms": 3200, "error": None},
                ],
                "state": "done",
                "code": 1,
                "message": "レビュー完了（指摘あり 1 件）" if ja else "Review finished (1 finding)",
                "findings": 1,
                "ms": 6000,
            }
        ],
        "log_unreadable": 0,
    }


def add_continuity_data(d, lang):
    ja = lang == "ja"
    rev = d["review"]
    first = d["hist"][0]
    prev = {
        **first,
        "id": "r0",
        "seq": 1,
        "created_at": ms(24, 18, 0, 0),  # 前日
        "findings": 4,
        "verified": 4,
        "total_tokens": 2400,
        "counts": {"open": 0, "done": 3, "wait": 1, "dismiss": 0},
    }
    first["seq"] = rev["seq"] = 2
    first["created_at"] = ms(25, 11, 30, 0)  # 本日
    d["hist"].insert(1, prev)
    rev["previous"] = {"id": "r0", "created_at": prev["created_at"], "findings": 4, "seq": 1}
    titles = (
        [
            ("ユーザー入力を SQL に直接つないでいる", "api/users.py"),
            ("レート制限超過時のステータスコードが違う", "middleware.py"),
            ("ログ出力の書式が揃っていない", "logger.py"),
            ("セッションの有効期限を確かめていない", "auth.py"),
        ]
        if ja
        else [
            ("Direct SQL string concatenation from user input", "api/users.py"),
            ("Rate limit status code mismatch", "middleware.py"),
            ("Inconsistent log format", "logger.py"),
            ("Session expiry is not checked", "auth.py"),
        ]
    )
    states = ["fix", "fix", "fix", "wait"]
    outcomes = ["persists", "gone", "gone", "not_reraised"]
    rev["fates"] = [
        {"from_review": "r0", "from_index": i, "title": t, "file": f, "state_before": states[i], "fate": outcomes[i]}
        for i, (t, f) in enumerate(titles)
    ]
    rev["carries"] = [{"from_review": "r0", "from_index": 0, "state": "fix", "exact": True}, None, None]


def get_demo_data(lang):
    d = copy.deepcopy(lp.make_demo_data())
    ja = lang == "ja"
    if ja:
        for f, (t, det) in zip(d["review"]["findings"], JA_FINDINGS):
            f["title"], f["detail"] = t, det
        for r, t in zip(d["recipes"]["recipes"], JA_RECIPES):
            r["title"] = t
        d["review"]["recipe"] = d["recipes"]["recipes"][0]
        d["providers"][0]["note"] = "自分の API キーで、直接送ります。中継のサーバはありません。"
        d["snap"]["excluded"] = d["review"]["snapshot"]["excluded"] = [{"path": "package-lock.json", "reason": "ロックファイル"}]
        d["hist"][0]["recipe_title"], d["hist"][1]["recipe_title"], d["hist"][2]["recipe_title"] = "セキュリティ", "潜在バグ", "例外処理"
        d["hist"][2]["provider_label"] = "Anthropic API"
        d["settings"]["values"]["language"] = "ja"
    add_continuity_data(d, lang)
    d["hook_status"] = make_hook_status(lang)
    return d


def build_mock_script(lang):
    data = get_demo_data(lang)
    handoff_escaped = lp.HANDOFF.replace("'", "\\'")
    s = f"""
window.__DATA = {json.dumps(data, ensure_ascii=False)};
window.__TAURI_INTERNALS__ = {{
  transformCallback: () => 0,
  invoke: async (cmd, args) => {{
    const D = window.__DATA;
    switch (cmd) {{
      case 'list_providers': return D.providers;
      case 'list_bases': return [
        {{value: 'HEAD', label: '{("コミット前の変更" if lang == "ja" else "Uncommitted changes")}', hint: '{("作業ツリーの変更" if lang == "ja" else "Changes in working tree")}'}},
        {{value: 'HEAD~1', label: '{("直前のコミット" if lang == "ja" else "Latest commit")}', hint: '{("直前のコミットと、未コミットの変更" if lang == "ja" else "Last commit and uncommitted")}'}}
      ];
      case 'list_recipes': return args && args.repo ? D.recipes : {{recipes: D.recipes.recipes.filter(r => r.kind !== 'tool'), warnings: []}};
      case 'preview_snapshot': return D.snap;
      case 'list_reviews': return D.hist;
      case 'get_review': return D.review;
      case 'hook_status': return D.hook_status;
      case 'get_usage': return D.usage;
      case 'get_settings': return D.settings;
      case 'get_report': return D.report;
      case 'repos_load': return {{repos: [{{id:'a',path:'/Users/me/Git/shop-api'}},{{id:'b',path:'/Users/me/Git/blog-engine'}},{{id:'c',path:'/Users/me/Git/dotfiles'}}], limit: 3, warnings: []}};
      case 'repo_status': return (args.paths||[]).map(p => ({{path: p, state: 'ok', root: null}}));
      case 'repo_data': return (args.paths||[]).map((p, i) => ({{path: p, reviews: 3, findings: 9, triaged: 5, memos: 2, open: [24, 7, 13][i] ?? 0}}));
      case 'byok_status': return {{supported:true,key_set:true,key_last4:'a1B2',key_error:null,model:'claude-sonnet-5-5',max_tokens:8192,config_error:null}};
      case 'build_handoff': return '{handoff_escaped}';
      case 'write_handoff': return {{text: '{handoff_escaped}', path: '/Users/me/Git/shop-api/.mrcodereview/out/handoff.md', rel: '.mrcodereview/out/handoff.md', count: 2, prompt: 'Read .mrcodereview/out/handoff.md and fix the findings listed under "Fix". Do not touch "Not now".'}};
      case 'set_triage': {{
        window.__TRI = window.__TRI || D.review.triage.slice();
        window.__TRI[args.index] = args.status;
        return {{triage: window.__TRI.slice(), notes: D.review.finding_notes}};
      }}
      case 'update_handling': return {{triage: D.review.triage, notes: D.review.finding_notes}};
      case 'update_finding_memo': return {{triage: D.review.triage, notes: D.review.finding_notes}};
      case 'toggle_finding_done': return {{triage: D.review.triage, notes: D.review.finding_notes}};
      default: return null;
    }}
  }},
}};
try {{
  localStorage.setItem('mrcr.repo', '/Users/me/Git/shop-api');
  localStorage.setItem('mrcr.base', 'HEAD');
  localStorage.setItem('mrcodereview.language', '{lang}');
}} catch (e) {{}}
"""
    return s


def free_port():
    with socket.socket() as k:
        k.bind(("127.0.0.1", 0))
        return k.getsockname()[1]


def capture(lang, port):
    print(f"\n==================== Capturing for {lang.upper()} ====================")
    mock = build_mock_script(lang)
    is_ja = lang == "ja"

    out_art = ROOT / "art"
    out_media = MRKTOOLS / "public/assets/mrcodereview/media"
    out_shots = MRKTOOLS / f"local/hero-shots/mrcodereview/{lang}"
    out_art.mkdir(parents=True, exist_ok=True)
    out_media.mkdir(parents=True, exist_ok=True)
    out_shots.mkdir(parents=True, exist_ok=True)

    with sync_playwright() as p:
        browser = p.chromium.launch(executable_path=EXE)

        # -------------------------------------------------------------
        # 1. runlog-feed: 初期画面 実行ログタイムライン ＆ 2階層インライン展開
        # -------------------------------------------------------------
        print(f"[{lang}] 1. runlog-feed (initial screen + inline expansion)...")
        # LP 用: 2400x1500 (1200x750 @2x)
        ctx = browser.new_context(
            viewport={"width": 1200, "height": 750},
            locale="ja-JP" if is_ja else "en-US",
            color_scheme="light",
            device_scale_factor=2,
        )
        page = ctx.new_page()
        page.add_init_script(mock)
        page.goto(f"http://127.0.0.1:{port}/")
        page.wait_for_selector(".topbar", timeout=15000)
        page.wait_for_timeout(800)

        # 実行ログカードの1つ目（最新レビュー）をクリックしてインライン展開
        card_btn = page.locator(".runlog-card .primary").first
        if card_btn.count() > 0:
            card_btn.click()
            page.wait_for_timeout(600)

        # LP 用 2400x1500
        path_lp_feed = out_media / ("feature-runlog.png" if is_ja else "feature-runlog-en.png")
        page.screenshot(path=str(path_lp_feed))
        print(f"  -> Saved LP runlog: {path_lp_feed}")

        # README 用: 2880x2200 (1440x1100 @2x)
        page.set_viewport_size({"width": 1440, "height": 1100})
        page.wait_for_timeout(400)
        path_art_feed = out_art / ("runlog-feed.png" if is_ja else "runlog-feed-en.png")
        page.screenshot(path=str(path_art_feed))
        print(f"  -> Saved README runlog: {path_art_feed}")
        ctx.close()

        # -------------------------------------------------------------
        # 2. hero: レビュー詳細画面（指摘カード・差分・つなぎ帯・仕分け）
        # -------------------------------------------------------------
        print(f"[{lang}] 2. hero (review detail screen)...")
        # LP 用: 2400x1500 (1200x750 @2x)
        ctx = browser.new_context(
            viewport={"width": 1200, "height": 750},
            locale="ja-JP" if is_ja else "en-US",
            color_scheme="light",
            device_scale_factor=2,
        )
        page = ctx.new_page()
        page.add_init_script(mock)
        page.goto(f"http://127.0.0.1:{port}/")
        page.wait_for_selector(".topbar", timeout=15000)
        page.wait_for_timeout(600)

        # 右パネルの履歴1件目をクリックして詳細画面へ遷移
        page.click(".history li >> nth=0 >> .hbody")
        page.wait_for_timeout(600)

        # LP 用 2400x1500
        path_lp_hero = out_media / ("hero-v2.png" if is_ja else "hero-v2-en.png")
        page.screenshot(path=str(path_lp_hero))
        print(f"  -> Saved LP hero: {path_lp_hero}")

        path_shots_hero = out_shots / "hero.png"
        page.screenshot(path=str(path_shots_hero))
        print(f"  -> Saved hero-shots: {path_shots_hero}")

        # README 用: 2880x2200 (1440x1100 @2x)
        page.set_viewport_size({"width": 1440, "height": 1100})
        page.wait_for_timeout(400)
        path_art_hero = out_art / ("hero.png" if is_ja else "hero-en.png")
        page.screenshot(path=str(path_art_hero))
        print(f"  -> Saved README hero: {path_art_hero}")

        ctx.close()
        browser.close()


def main():
    port = free_port()
    print(f"Starting Vite on port {port}...")
    vite = subprocess.Popen(
        ["npx", "vite", "--port", str(port), "--strictPort", "--host", "127.0.0.1"],
        cwd=APP,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    try:
        for _ in range(120):
            try:
                urllib.request.urlopen(f"http://127.0.0.1:{port}/", timeout=2).read()
                break
            except Exception:
                time.sleep(0.5)
        else:
            sys.exit("Vite が起動しませんでした")
        print("Vite started successfully.")

        capture("ja", port)
        capture("en", port)
        print("\nAll screenshots generated successfully!")
    finally:
        vite.terminate()


if __name__ == "__main__":
    main()
