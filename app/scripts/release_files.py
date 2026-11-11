#!/usr/bin/env python3
"""リリースで、サイトに置くファイルの作成と、古い版の片付け（`scripts/release.sh` から呼ぶ。試験は test_release_files.py）。

  release_files.py write SITE VERSION DMG TGZ BASE_URL SIG_FILE NAME     update.json（自動更新の案内）と latest.json（ダウンロードの案内）を書く
  release_files.py prune SITE NAME KEEP --protect VERSION               新しい KEEP 版だけを残して、古い DMG・更新用 tar.gz を消す
  release_files.py check-config CONF NAME IDENT SLUG                    tauri.conf.json が、期待どおり（名前・identifier・アイコン・更新の宛先）かを検査する

- 版の並びは、数として比べる（0.10.0 は 0.9.0 より新しい）。
- KEEP は 1 以上。0 だと、最新版まで消えてしまうので、断る。
- --protect の版は、新しい KEEP 版に入っていなくても、消さない（いま置いた版を、古い版として消さないため）。
"""
import argparse
import datetime
import hashlib
import json
import os
import re
import sys


def version_key(text):
    return tuple(int(x) for x in text.split("."))


def prune_old_versions(site, name, keep, protect=None):
    """消したファイル名の一覧を返す。名前が、`<name>_<版>_aarch64.dmg` か `<name>_<版>.app.tar.gz` のものだけを対象にする。"""
    if keep < 1:
        raise ValueError("KEEP は 1 以上にしてください（0 だと、最新版まで消えます）")
    pat = re.compile(r"^%s_(\d+(?:\.\d+)*)(?:_aarch64\.dmg|\.app\.tar\.gz)$" % re.escape(name))
    versions = {}
    for f in sorted(os.listdir(site)):
        m = pat.match(f)
        if m:
            versions.setdefault(version_key(m.group(1)), []).append(f)
    protected = version_key(protect) if protect else None
    removed = []
    for v in sorted(versions, reverse=True)[keep:]:
        if v == protected:
            continue
        for f in versions[v]:
            os.remove(os.path.join(site, f))
            removed.append(f)
    return removed


def write_update_files(site, version, dmg, tgz, base_url, sig_file, name, now=None):
    """update.json と latest.json を書く。(update, latest) を返す。DMG は、site の中にあること（大きさと sha256 を測る）。"""
    now = now or datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    with open(sig_file) as fh:
        sig = fh.read().strip()
    update = {
        "version": version,
        "notes": f"{name} {version}",
        "pub_date": now,
        "platforms": {"darwin-aarch64": {"signature": sig, "url": f"{base_url}/{tgz}"}},
    }
    path = os.path.join(site, dmg)
    with open(path, "rb") as fh:
        data = fh.read()
    latest = {
        "version": version,
        "tag": f"v{version}",
        "published": now,
        "files": [{"name": dmg, "size": len(data), "sha256": hashlib.sha256(data).hexdigest()}],
    }
    for fname, doc in (("update.json", update), ("latest.json", latest)):
        with open(os.path.join(site, fname), "w") as fh:
            json.dump(doc, fh, indent=2)
            fh.write("\n")
    return update, latest


def load_json(path):
    with open(path, encoding="utf-8") as fh:
        return json.load(fh)


def check_config(conf_path, name, ident, slug):
    """tauri.conf.json が、期待どおりかを検査する。問題の一覧を返す（空なら、通過）。
    アイコンの相対パスは、CONF のある場所からの相対。"""
    conf = load_json(conf_path)
    problems = []
    if conf.get("productName") != name:
        problems.append("productName が %r です（期待: %r）" % (conf.get("productName"), name))
    if conf.get("identifier") != ident:
        problems.append("identifier が %r です（期待: %r）" % (conf.get("identifier"), ident))
    icons = (conf.get("bundle") or {}).get("icon") or []
    if not icons:
        problems.append("bundle.icon がありません")
    for icon in icons:
        if not os.path.isfile(os.path.join(os.path.dirname(conf_path), icon)):
            problems.append("アイコンのファイルがありません: %s" % icon)
    endpoints = ((conf.get("plugins") or {}).get("updater") or {}).get("endpoints") or []
    if not endpoints:
        problems.append("plugins.updater.endpoints がありません")
    for e in endpoints:
        if not e.endswith("/downloads/%s/update.json" % slug):
            problems.append("更新の宛先が、期待のものではありません: %s（期待: .../downloads/%s/update.json）" % (e, slug))
    return problems


def main(argv):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    w = sub.add_parser("write")
    for a in ("site", "version", "dmg", "tgz", "base_url", "sig_file", "name"):
        w.add_argument(a)
    p = sub.add_parser("prune")
    p.add_argument("site")
    p.add_argument("name")
    p.add_argument("keep", type=int)
    p.add_argument("--protect")
    c = sub.add_parser("check-config")
    for a in ("conf", "name", "ident", "slug"):
        c.add_argument(a)
    args = ap.parse_args(argv)
    if args.cmd == "check-config":
        problems = check_config(args.conf, args.name, args.ident, args.slug)
        for pr in problems:
            print("NG: %s: %s" % (args.name, pr), file=sys.stderr)
        if not problems:
            print("OK: %s の設定（productName・identifier・アイコン・更新の宛先）" % args.name)
        return 1 if problems else 0
    if args.cmd == "write":
        write_update_files(args.site, args.version, args.dmg, args.tgz, args.base_url, args.sig_file, args.name)
        return 0
    try:
        removed = prune_old_versions(args.site, args.name, args.keep, args.protect)
    except ValueError as e:
        print(f"NG: {e}", file=sys.stderr)
        return 1
    for f in removed:
        print("消した:", f)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
