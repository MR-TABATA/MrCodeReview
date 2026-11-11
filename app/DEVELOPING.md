# 開発メモ（GUI: `app/`）

GUI は、Tauri v2（Rust ＋ TypeScript / Vite）。エンジン（`crates/mrcr-core`）と CLI（`crates/mrcr`）は、
同じリポジトリのルートにある。Cargo のワークスペースに `app/src-tauri` も入っているので、ビルドは、リポジトリのルートからでも `app/` からでもよい。

## 動かす

```sh
cd app
npm ci
# 手元の本物のデータと分けるため、データの置き場を、このリポジトリの ignore 済みの場所に向ける
MRCR_DATA_DIR="$PWD/.dev/data" npm run tauri dev
```

- `MRCR_DATA_DIR` を付けないと、本物のデータ（`~/Library/Application Support/com.hitoshi.mrcodereview/`。旧名 `com.hitoshi.mrkcodereview/` があれば、初回の起動で、ここへ移る）を使う。開発中は付ける。
- 試験用の架空リポジトリは、`npm run demo-repo`（`scripts/make-demo-repo.sh`）が作る。置き場は `.dev/`（ignore 済み）。
- キーチェーンの項目は、配布版と同じ（`com.hitoshi.mrcodereview`）。開発ビルドでも、保存済みの API キーが読まれる。

## 試験

```sh
cargo test --workspace          # リポジトリのルートで。core・CLI・GUI（app/src-tauri）
cd app && npm test              # vitest
cd app && npm run typecheck     # tsc
cd app && npx eslint src        # 既知の 5 件が出る（Mrk から引き継いだもの）
```

## 構成の注意

- 観点（レビューの文面）は、リポジトリ直下の `recipes/` が正（`mrcr-core` が取り込む）。GUI 側には持たない。
- データの置き場の決め方・移行・同時起動のロックは、`crates/mrcr-core/src/datadir.rs` と `applock.rs` が唯一の実装（GUI と CLI が同じものを呼ぶ）。置き場の名前は、アプリの `identifier` と同じ `com.hitoshi.mrcodereview`。
- 旧名 `com.hitoshi.mrkcodereview` からは、ロックを取って 1 回だけ rename で移し、旧名に新へのリンクを**残す**（旧版が割れないように）。リンクは、旧版が使われなくなったら消す（`datadir::LEGACY_LINK_REMOVAL`。コードでは消さない）。
- 別のアプリ（将来の有償版など）は、自分の置き場を持つ。OSS 版のデータは、有償版が「インポートするか」を聞いてコピーする。
- 設定ファイルの `tool_timeout_secs`・`max_tool_files`・`max_raw_bytes` は、廃止した linter の項目。古い設定ファイルを読めるよう、受け取って無視する。書き出さない。
- 過去の記録に残る、`kind: tool`（linter の結果）の表示は、残している。

## 未整備（移行中）

次は、まだ、このリポジトリに移していない。

- **リリース手順**: 署名・公証・配布の置き場への配置（旧 `release.sh`）。
- **自動更新の宛先**: `tauri.conf.json` の `endpoints` は、旧来の `mrktools.app/downloads/mrcodereview/update.json` のまま。
- **CI**: GUI のビルド・試験。
- **`THIRD-PARTY-NOTICES.md`**: 生成スクリプトは `scripts/gen-third-party-notices.py`。依存を変えたので、作り直しが要る。
