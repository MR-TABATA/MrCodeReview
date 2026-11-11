#!/bin/sh
# MrCodeReview（GUI）のリリース（Apple Silicon・手元で署名と公証）。
#   ビルド → Developer ID で署名 → 公証 → 更新用 tar.gz と署名 → .dmg → 公証
#   → サイトの置き場に、DMG・更新用ファイル・update.json・latest.json を置く。
# コミット・push・公開は、しない（置いたあと、サイトのリポジトリで commit して push するのは、人の操作）。
# CLI（mrcr）の配布は、これとは別（リポジトリ直下の scripts/release.sh と .github/workflows/release.yml）。
#
# 使い方（app/ から）:
#   sh scripts/release.sh --check    事前の検査だけ（ビルド・公証・配置をしない。何も書かない）
#   sh scripts/release.sh            リリースを作る（版は src-tauri/tauri.conf.json の version）
# 環境変数（既定値あり）:
#   SITE_DIR=...         出力先（既定 ~/Git/MrkTools/public/downloads/mrcodereview。自動更新の宛先 mrktools.app/downloads/mrcodereview/ になる）
#   NOTARY_PROFILE=...   notarytool のキーチェーンプロファイル（既定 mrkcodereview）
#   SIGN_IDENTITY=...    署名する Developer ID（既定 ログイン側の証明書のハッシュ）
#   KEEP_VERSIONS=...    サイトに残す版の数（既定 3。古い DMG と tar.gz から消す）
#   TAURI_KEY=... TAURI_KEY_PASSWORD_FILE=...   更新の秘密鍵とそのパスワードのファイル（既定 ~/.tauri/mrkcodereview.key と .password。リポジトリに入れない）
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"      # app/
REPO="$(cd "$ROOT/.." && pwd)"                 # リポジトリの根（Cargo のワークスペース。ビルドの出力は $REPO/target）
NAME="MrCodeReview"
SLUG="mrcodereview"
IDENT="com.hitoshi.mrcodereview"
SITE_DIR="${SITE_DIR:-$HOME/Git/MrkTools/public/downloads/$SLUG}"
BASE_URL="https://mrktools.app/downloads/$SLUG"
NOTARY_PROFILE="${NOTARY_PROFILE:-mrkcodereview}"
# 同名の証明書が System とログインの両方にあり、名前だと曖昧になるので、ログイン側のハッシュで指す。
# （以前の CB47… は 2027-02-01 に失効する。2CC8… を使う）
SIGN_IDENTITY="${SIGN_IDENTITY:-2CC8414D04AECA2A8FBA4D57754C624F6ED4BE1A}"
KEEP_VERSIONS="${KEEP_VERSIONS:-3}"
KEY="${TAURI_KEY:-$HOME/.tauri/mrkcodereview.key}"
KEY_PASS_FILE="${TAURI_KEY_PASSWORD_FILE:-$HOME/.tauri/mrkcodereview.key.password}"
# リリースのたびに外から取得せず、package-lock.json で固定した、手元の Tauri CLI を使う
TAURI="npx --no-install tauri"
TARGET="aarch64-apple-darwin"

CHECK_ONLY=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --check) CHECK_ONLY=1 ;;
    *) echo "不明な引数: ${1}（使えるのは --check だけ）" >&2; exit 2 ;;
  esac
  shift
done

fail=0
bad() { echo "NG: $1" >&2; fail=1; }
ok() { echo "OK: $1"; }

cd "$ROOT"
jsonver() { python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["version"])' "$1"; }
VERSION="$(jsonver src-tauri/tauri.conf.json)"
echo "== 事前の検査（v${VERSION}）"

# 1. 版が、3 か所で同じ
PKG="$(jsonver package.json)"
CRATE="$(sed -n 's/^version = "\(.*\)"/\1/p' src-tauri/Cargo.toml | head -1)"
if [ "$VERSION" = "$PKG" ] && [ "$VERSION" = "$CRATE" ]; then ok "版が一致（tauri.conf.json / package.json / Cargo.toml: ${VERSION}）"
else bad "版が食い違っています: tauri.conf.json=$VERSION package.json=$PKG Cargo.toml=$CRATE"; fi
case "$VERSION" in 0.*) bad "版が 0.x です（${VERSION}）。1.0.0 以上にしてから" ;; esac

# 1b. 残す版の数は、1 以上の整数（0 だと、最新版まで消える）
case "$KEEP_VERSIONS" in
  ''|*[!0-9]*|0) bad "KEEP_VERSIONS は、1 以上の整数にしてください（いまの値: ${KEEP_VERSIONS}）" ;;
  *) ok "サイトに残す版の数: ${KEEP_VERSIONS}" ;;
esac

# 1c. 名前・identifier・アイコンのファイル・更新の宛先
if python3 scripts/release_files.py check-config src-tauri/tauri.conf.json "$NAME" "$IDENT" "$SLUG"; then :; else fail=1; fi

# 2. 作業ツリーが、きれい（配ったものと、ソースが 1 対 1 になるように）
if [ -z "$(git -C "$REPO" status --porcelain)" ]; then ok "未コミットの変更なし"; else bad "未コミットの変更があります。コミットしてから"; fi

# 3. 秘密と、公証のプロファイル
if [ -f "$KEY" ] && [ -f "$KEY_PASS_FILE" ]; then ok "更新の署名鍵がある"; else bad "更新の署名鍵がありません: ${KEY}（と .password）"; fi
if security find-identity -v -p codesigning | grep -q "$SIGN_IDENTITY"; then ok "署名の証明書がある"; else bad "署名の証明書が見つかりません: $SIGN_IDENTITY"; fi
# 公証のプロファイルは、Apple に問い合わせないと確かめられない。--check では確かめない（本番の最初に確かめる）
echo "（未確認）公証のプロファイル: $NOTARY_PROFILE は、リリース本番の最初に確かめます"

# 4. 第三者のライセンス表示が、いまの依存と合っている（依存を変えたら、作り直す）
if python3 scripts/gen-third-party-notices.py --check >/dev/null 2>&1; then ok "第三者のライセンス表示は最新"; else bad "THIRD-PARTY-NOTICES.md が古い、または抜けがある（python3 scripts/gen-third-party-notices.py）"; fi

# 5. テスト
echo "== テスト"
if ( cd "$REPO" && cargo test --workspace --quiet ) >/dev/null 2>&1; then ok "cargo test --workspace"; else bad "cargo test --workspace が失敗"; fi
if npx --no-install vitest run >/dev/null 2>&1; then ok "vitest"; else bad "vitest が失敗"; fi
if npx --no-install tsc --noEmit >/dev/null 2>&1; then ok "tsc"; else bad "tsc が失敗"; fi
if python3 -I -m unittest discover -s scripts -p 'test_release_files.py' >/dev/null 2>&1; then ok "scripts の試験（release_files）"; else bad "scripts の試験（release_files）が失敗"; fi

if [ "$fail" -ne 0 ]; then echo "事前の検査: 失敗（リリースしない）" >&2; exit 1; fi
echo "事前の検査: 通過"
[ "$CHECK_ONLY" -eq 1 ] && exit 0

KEY_PASS="$(cat "$KEY_PASS_FILE")"

OUT="$REPO/target/release-work"
APP_DIR="$REPO/target/$TARGET/release/bundle/macos"
APP="$APP_DIR/$NAME.app"
BIN="$REPO/target/$TARGET/release/mrcodereview"
DMG_NAME="${NAME}_${VERSION}_aarch64.dmg"
TGZ_NAME="${NAME}_${VERSION}.app.tar.gz"

echo "== 公証のプロファイルの確認（Apple に問い合わせる。履歴の一覧を読むだけ）"
xcrun notarytool history --keychain-profile "$NOTARY_PROFILE" >/dev/null 2>&1 \
  || { echo "NG: 公証のプロファイルを使えません: ${NOTARY_PROFILE}（xcrun notarytool store-credentials $NOTARY_PROFILE --team-id G8E27F3U7R）" >&2; exit 1; }

rm -rf "$OUT" && mkdir -p "$OUT" "$SITE_DIR"

echo "== ビルド（${NAME}・Apple Silicon）v$VERSION"
npm run build >/dev/null
printf '{"bundle":{"createUpdaterArtifacts":true}}\n' > "$OUT/config.json"
APPLE_SIGNING_IDENTITY="$SIGN_IDENTITY" \
TAURI_SIGNING_PRIVATE_KEY="$(cat "$KEY")" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$KEY_PASS" \
  $TAURI build --target "$TARGET" --bundles app --config "$OUT/config.json"

echo "== 検査（名前・identifier・アイコン・更新の宛先が、期待どおり）"
PLIST="$APP/Contents/Info.plist"
[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleName' "$PLIST")" = "$NAME" ] \
  || { echo "NG: .app の名前（CFBundleName）が違います" >&2; exit 1; }
[ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$PLIST")" = "$IDENT" ] \
  || { echo "NG: .app の identifier が違います" >&2; exit 1; }
cmp -s "$APP/Contents/Resources/icon.icns" "src-tauri/icons/free/icon.icns" \
  || { echo "NG: .app のアイコンが、icons/free/ のものではありません" >&2; exit 1; }
strings "$BIN" | grep -q "downloads/${SLUG}/update.json" \
  || { echo "NG: 実行ファイルに、更新の宛先（downloads/${SLUG}/update.json）がありません" >&2; exit 1; }
echo "OK: 設定が、.app と実行ファイルに反映されている"

echo "== 配布ビルドに、開発用の差し替えが入っていないこと"
if strings "$BIN" | grep -E -q 'MrCodeReview-dev|MrkCodeReview-dev|com\.hitoshi\.mrcodereview\.dev|MRCR_DEV_ENTITLED|author-providers'; then
  echo "NG: 開発用の名前・環境変数・機能の痕跡が、配布ビルドに入っています。配布しない" >&2
  exit 1
fi
echo "OK: 開発用の差し替えは、入っていない"

echo "== 署名の確認"
codesign --verify --deep --strict "$APP"

echo "== アプリの公証"
ditto -c -k --keepParent "$APP" "$OUT/app.zip"
xcrun notarytool submit "$OUT/app.zip" --keychain-profile "$NOTARY_PROFILE" --wait
xcrun stapler staple "$APP"
spctl -a -vvv -t exec "$APP"

echo "== 更新用の tar.gz に署名（公証済みの .app を固める）"
# COPYFILE_DISABLE=1: macOS の tar は、拡張属性を「._ で始まるファイル」として混ぜる。入っていると、自動更新の展開が失敗する（SyncLane 0.2.0 で起きた）
COPYFILE_DISABLE=1 tar -czf "$OUT/$TGZ_NAME" -C "$APP_DIR" "$NAME.app"
python3 - "$OUT/$TGZ_NAME" <<'PY'
import sys, tarfile
names = tarfile.open(sys.argv[1]).getnames()
bad = [n for n in names if any(part.startswith("._") for part in n.split("/"))]
if bad:
    sys.exit("更新用の tar.gz に ._ のファイルが混ざっています: %s" % bad)
print("更新用の tar.gz: %d 項目、._ なし" % len(names))
PY
TAURI_SIGNING_PRIVATE_KEY="$(cat "$KEY")" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$KEY_PASS" \
  $TAURI signer sign "$OUT/$TGZ_NAME"

echo "== dmg"
STAGE="$OUT/stage"; mkdir -p "$STAGE"
ditto "$APP" "$STAGE/$NAME.app"
ln -s /Applications "$STAGE/Applications"
hdiutil create -volname "$NAME" -srcfolder "$STAGE" -ov -format UDZO "$OUT/$DMG_NAME" >/dev/null
codesign --sign "$SIGN_IDENTITY" --timestamp "$OUT/$DMG_NAME"
xcrun notarytool submit "$OUT/$DMG_NAME" --keychain-profile "$NOTARY_PROFILE" --wait
xcrun stapler staple "$OUT/$DMG_NAME"
spctl -a -t open --context context:primary-signature -vvv "$OUT/$DMG_NAME"

echo "== 配置: $SITE_DIR"
cp "$OUT/$DMG_NAME" "$OUT/$TGZ_NAME" "$SITE_DIR/"
python3 scripts/release_files.py write "$SITE_DIR" "$VERSION" "$DMG_NAME" "$TGZ_NAME" "$BASE_URL" "$OUT/$TGZ_NAME.sig" "$NAME"

echo "== 古い版を片付ける（新しい $KEEP_VERSIONS 版を残す）"
# いま置いた版（update.json が指す版）は、新しい版に入っていなくても、消さない
python3 scripts/release_files.py prune "$SITE_DIR" "$NAME" "$KEEP_VERSIONS" --protect "$VERSION"
rm -rf "$OUT"
echo "完了: $DMG_NAME"
echo "次: サイトのリポジトリ（$SITE_DIR の持ち主）で内容を確かめて、commit → push（公開）。この script は、commit も push もしていません。"
