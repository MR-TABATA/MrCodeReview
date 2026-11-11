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

## リリース（GUI・Apple Silicon・手元で署名と公証）

```sh
cd app
sh scripts/release.sh --check    # 事前の検査だけ。ビルド・公証・配置をしない（版・設定・未コミット・鍵・証明書・テスト）
sh scripts/release.sh            # リリースを作る（版は src-tauri/tauri.conf.json の version）
```

ビルド → Developer ID で署名 → 公証 → 更新用 tar.gz と署名 → DMG → 公証 → サイトの置き場に配置。**commit・push・公開は、しない**
（置いたあと、サイトのリポジトリで内容を確かめて commit → push するのは、人の操作）。CLI（`mrcr`）の配布は別（リポジトリ直下の `scripts/release.sh`・`.github/workflows/release.yml`）。

- 版は、`tauri.conf.json`・`package.json`・`src-tauri/Cargo.toml` の 3 か所を同じにする（食い違うと `--check` が止める）。
- 配置先の既定は `~/Git/MrkTools/public/downloads/mrcodereview/`（`SITE_DIR` で変えられる）。自動更新の宛先は `https://mrktools.app/downloads/mrcodereview/update.json`。**この宛先は、配布済みの MrkCodeReview 1.x（Free 殻）が持っているものと同じ**なので、変えない（変えると、配布済みのアプリが更新を受け取れなくなる）。
- サイトに残すのは新しい 3 版まで（`KEEP_VERSIONS`）。古い DMG と更新用 tar.gz は、スクリプトが消す。
- 署名の証明書は、`SIGN_IDENTITY`（既定 `2CC8414D…`。以前の `CB47…` は 2027-02-01 に失効する）。
- Tauri の CLI は、`package-lock.json` で固定した手元のもの（`npx --no-install tauri`）。リリースのたびに外から取得しない。
- 初回だけ、手元で用意する（どちらも、リポジトリには入れない）:

```sh
xcrun notarytool store-credentials mrkcodereview --team-id G8E27F3U7R      # Apple ID と、アプリ用パスワード
mkdir -p ~/.tauri && npx tauri signer generate -w ~/.tauri/mrkcodereview.key
printf '%s' '（鍵のパスワード）' > ~/.tauri/mrkcodereview.key.password && chmod 600 ~/.tauri/mrkcodereview.key.password
```

**未検証**: 移行元の `release.sh` から、殻の切り替え・Polar・CLI の LLM 検査を外して移した。事前の検査（`--check`）は通ることを確かめたが、
ビルド・公証・配置の本番は、まだ一度も走らせていない（初回は、配置先を一時フォルダにして試す: `SITE_DIR=/tmp/…` ではなく、ignore 済みの場所で）。

## 未整備（移行中）

次は、まだ、このリポジトリに移していない。

- **リリースの本番**: 上の手順は、まだ本番を走らせていない（`--check` だけ通る）。
- **CI**: `.github/workflows/ci.yml`（Rust の試験は macOS・画面側は Node）を足した。GitHub 上では、まだ走らせていない（最初の push で確かめる）。CLI の配布（`release.yml`）の試験は、CLI と core だけに絞った。
- **`THIRD-PARTY-NOTICES.md`**: 2026-11-11 に作り直した（`python3 scripts/gen-third-party-notices.py --check` で確かめられる）。依存を変えたら、作り直す。
