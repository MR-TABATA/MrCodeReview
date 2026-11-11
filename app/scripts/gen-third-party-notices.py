#!/usr/bin/env python3
"""配布物に入る依存ライブラリ（Rust・npm の本番依存）のライセンス表示を、THIRD-PARTY-NOTICES.md に書き出す。

- Rust: macOS（aarch64）向けの、普通の依存だけ（開発用・ビルド用だけの依存は入れない）。ライセンス文は、各クレートのソースの
  最上位にある LICENSE / COPYING / NOTICE などのファイルから、そのまま写す。同じ文面は 1 つにまとめて、使うクレートを並べる。
- npm: package.json の dependencies（本番依存）だけ。
- ライセンス文のファイルが見つからないクレートは、黙って飛ばさず、末尾の「ライセンス文のファイルが見つからなかったもの」に書く。
  --check のときは、1 つでもあれば失敗する（配布しない）。
使い方:  python3 scripts/gen-third-party-notices.py          # 書き出す
         python3 scripts/gen-third-party-notices.py --check  # いまのファイルが最新か、抜けが無いかだけを確かめる（書かない）
"""
import hashlib
import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "THIRD-PARTY-NOTICES.md")
TARGET = "aarch64-apple-darwin"
NAMES = ("LICENSE", "LICENCE", "COPYING", "NOTICE", "UNLICENSE")


MIT_TEXT = """MIT License

Copyright (c) {holders}

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE."""

BSD3_TEXT = """BSD 3-Clause License

Copyright (c) {holders}
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
   contributors may be used to endorse or promote products derived from
   this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE."""


def run(cmd, cwd):
    return subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, check=True).stdout


def license_files(d):
    out = []
    for f in sorted(os.listdir(d)):
        if f.upper().startswith(NAMES) and os.path.isfile(os.path.join(d, f)):
            out.append(f)
    return out


def read(path):
    with open(path, encoding="utf-8", errors="replace") as fh:
        return fh.read().strip("\n")


def rust_packages():
    m = json.loads(run(["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", TARGET], os.path.join(ROOT, "src-tauri")))
    nodes = {n["id"]: n for n in m["resolve"]["nodes"]}
    pk = {p["id"]: p for p in m["packages"]}
    seen, stack = set(), [m["resolve"]["root"]]
    while stack:
        i = stack.pop()
        if i in seen:
            continue
        seen.add(i)
        for d in nodes[i]["deps"]:
            kinds = d["dep_kinds"]
            if all(k["kind"] in ("dev", "build") for k in kinds):
                continue
            stack.append(d["pkg"])
    root = m["resolve"]["root"]
    return [pk[i] for i in sorted(seen, key=lambda x: (pk[x]["name"], pk[x]["version"])) if i != root]


def fallback_text(lic, p, groups):
    """ライセンス文のファイルが無いクレートのために、標準の文面を返す（None なら補えない）。"""
    holders = ", ".join(a.split("<")[0].strip() for a in (p.get("authors") or []) if a.strip()) or f'the {p["name"]} developers'
    if "MIT" in lic:
        return MIT_TEXT.format(holders=holders)
    if "BSD-3-Clause" in lic:
        return BSD3_TEXT.format(holders=holders)
    key = "Apache License" if "Apache-2.0" in lic else "Mozilla Public License" if "MPL-2.0" in lic else None
    if key:
        for g in groups.values():
            if key in g["text"][:400] and "Version 2" in g["text"][:800]:
                return g["text"]
    return None


def collect():
    groups = {}   # ハッシュ -> {"text":..., "users":[...]}
    missing = []
    deferred = []
    supplied = []
    mpl = []
    for p in rust_packages():
        d = os.path.dirname(p["manifest_path"])
        files = license_files(d)
        label = f'{p["name"]} {p["version"]}'
        lic = p.get("license") or "（記載なし）"
        if "MPL" in lic:
            mpl.append((label, p.get("repository") or "", lic))
        if not files:
            deferred.append((p, label, lic))
            continue
        for f in files:
            text = read(os.path.join(d, f))
            h = hashlib.sha256(text.encode()).hexdigest()
            g = groups.setdefault(h, {"text": text, "users": []})
            g["users"].append(f'{label}（{lic}）')
    for p, label, lic in deferred:
        text = fallback_text(lic, p, groups)
        if text is None:
            missing.append((label, lic, p.get("repository") or ""))
            continue
        supplied.append(label)
        h = hashlib.sha256(text.encode()).hexdigest()
        g = groups.setdefault(h, {"text": text, "users": []})
        g["users"].append(f'{label}（{lic}。ソースに文面のファイルが無いため、標準の文面で補った）')
    npm = []
    pkg = json.load(open(os.path.join(ROOT, "package.json")))
    for name in sorted((pkg.get("dependencies") or {})):
        d = os.path.join(ROOT, "node_modules", name)
        meta = json.load(open(os.path.join(d, "package.json")))
        label = f'{name} {meta["version"]}'
        lic = meta.get("license") or "（記載なし）"
        files = license_files(d)
        if not files:
            missing.append((label, lic, ""))
            continue
        for f in files:
            text = read(os.path.join(d, f))
            h = hashlib.sha256(text.encode()).hexdigest()
            g = groups.setdefault(h, {"text": text, "users": []})
            g["users"].append(f'{label}（{lic}, npm）')
        npm.append(label)
    return groups, missing, mpl, npm, supplied


def render():
    groups, missing, mpl, npm, supplied = collect()
    n_users = sum(len(set(g["users"])) for g in groups.values())
    o = []
    o.append("# 第三者のソフトウェアのライセンス表示（Third-party notices）\n")
    o.append("MrCodeReview は、次のオープンソースのソフトウェアを含みます。それぞれのライセンスの文面を、以下に、そのまま示します。")
    o.append("同じ文面のものは、まとめて、使っているライブラリを並べました。\n")
    o.append("This product includes the open-source software listed below. Each license text is reproduced as published; libraries that share an identical text are grouped.\n")
    o.append("このファイルは、`scripts/gen-third-party-notices.py` が作ります（手で直さない）。依存を変えたら、作り直します。\n")
    o.append("## MPL-2.0 のライブラリについて（Mozilla Public License 2.0）\n")
    o.append("次のライブラリは MPL-2.0 です。**MrCodeReview は、これらのソースを変更せずに、そのまま使っています。** ソースは、それぞれの公開の場所で入手できます（クレートのソースは crates.io、リポジトリは下記）。\n")
    for label, repo, lic in mpl:
        o.append(f"- {label}（{lic}）{(' — ' + repo) if repo else ''}")
    o.append("")
    if supplied:
        o.append(f"## 標準の文面で補ったもの（{len(supplied)} 件）\n")
        o.append("次のライブラリは、ソースにライセンス文のファイルを同梱していません。宣言されたライセンスの標準の文面（SPDX）に、`Cargo.toml` の作者名を添えて示します（宣言が複数のライセンスの選択のときは、MIT を選びます）。\n")
        o.extend(f"- {name}" for name in supplied)
        o.append("")
    o.append("## ライセンスの文面\n")
    o.append(f"（Rust の依存と npm の本番依存の合計 {n_users} 件。文面は {len(groups)} 種類）\n")
    for i, (h, g) in enumerate(sorted(groups.items(), key=lambda kv: (-len(kv[1]["users"]), kv[0])), 1):
        users = sorted(set(g["users"]))
        o.append(f"### {i}. 使っているライブラリ（{len(users)} 件）\n")
        o.append("<details><summary>ライブラリの一覧</summary>\n")
        o.extend(f"- {u}" for u in users)
        o.append("\n</details>\n")
        fence = "`" * 4
        o.append(f"{fence}text\n{g['text']}\n{fence}\n")
    o.append("## ライセンス文のファイルが見つからなかったもの\n")
    if missing:
        o.append("ソースの最上位に、ライセンス文のファイルが無かったものです。ライセンスの種類は、各ライブラリの宣言（Cargo.toml / package.json）によります。\n")
        o.extend(f"- {name}（{lic}）{(' — ' + repo) if repo else ''}" for name, lic, repo in missing)
    else:
        o.append("なし。")
    o.append("")
    return "\n".join(o), missing


def main():
    text, missing = render()
    if "--check" in sys.argv:
        ok = True
        cur = open(OUT, encoding="utf-8").read() if os.path.exists(OUT) else ""
        if cur != text:
            print("NG: THIRD-PARTY-NOTICES.md が古い、または無い（python3 scripts/gen-third-party-notices.py で作り直す）", file=sys.stderr)
            ok = False
        if missing:
            print(f"NG: ライセンス文のファイルが見つからないものが {len(missing)} 件", file=sys.stderr)
            ok = False
        if ok:
            print("OK: 第三者のライセンス表示は最新で、抜けなし")
        sys.exit(0 if ok else 1)
    with open(OUT, "w", encoding="utf-8") as fh:
        fh.write(text)
    print(f"書き出し: {OUT}（{len(text)//1024} KB）。ライセンス文のファイルが見つからなかったもの: {len(missing)} 件")
    for name, lic, _repo in missing:
        print(f"  - {name}（{lic}）")


main()
