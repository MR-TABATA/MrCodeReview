#!/bin/sh
# 配布物を作る: universal binary（arm64 + x86_64）を 1 つの zip に。
#
#     sh scripts/release.sh            # 版は最新のタグから（v0.1.0 → 0.1.0）
#     sh scripts/release.sh 0.1.0      # 版を指定
#
# 出力: dist/mrcr-<版>-macos.zip と、その sha256（Homebrew の formula に貼る）。
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

VERSION="${1:-$(git describe --tags --abbrev=0 2>/dev/null | sed 's/^v//' || true)}"
if [ -z "$VERSION" ]; then
  # タグが無ければ crates/mrcr/Cargo.toml から読む
  VERSION=$(grep '^version = ' crates/mrcr/Cargo.toml | head -n 1 | sed 's/version = "\(.*\)"/\1/')
fi

if [ -z "$VERSION" ]; then
  echo "版が分からない。引数で渡すこと（例: sh scripts/release.sh 0.1.0）" >&2
  exit 1
fi

CARGO_VER=$(grep '^version = ' crates/mrcr/Cargo.toml | head -n 1 | sed 's/version = "\(.*\)"/\1/')
if [ "$CARGO_VER" != "$VERSION" ]; then
  echo "crates/mrcr/Cargo.toml は ${CARGO_VER}、配る版は ${VERSION}。先に Cargo.toml を直すこと" >&2
  exit 1
fi

echo "==> Building universal binary for mrcr v$VERSION..." >&2

# Apple Silicon と Intel Mac の両ターゲットをビルド
cargo build --release --target aarch64-apple-darwin -p mrcr 1>&2
cargo build --release --target x86_64-apple-darwin -p mrcr 1>&2

ARM="target/aarch64-apple-darwin/release/mrcr"
X86="target/x86_64-apple-darwin/release/mrcr"

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

lipo -create "$ARM" "$X86" -output "$STAGE/mrcr"
chmod +x "$STAGE/mrcr"

mkdir -p dist
ZIP="dist/mrcr-$VERSION-macos.zip"
rm -f "$ZIP"
(cd "$STAGE" && zip -qr "$ROOT/$ZIP" mrcr)

# 配布物の動作確認（Homebrew と同様に解凍して実行確認）
CHECK="$(mktemp -d)"
trap 'rm -rf "$STAGE" "$CHECK"' EXIT

unzip -q "$ZIP" -d "$CHECK"
for SHELL_NAME in bash zsh fish; do
  if ! "$CHECK/mrcr" --completions "$SHELL_NAME" >/dev/null 2>"$CHECK/err"; then
    echo "配布物の検査に失敗: mrcr --completions $SHELL_NAME" >&2
    cat "$CHECK/err" >&2
    rm -f "$ZIP"
    exit 1
  fi
done

GOT="$("$CHECK/mrcr" --version)"
if [ "$GOT" != "mrcr $VERSION" ]; then
  echo "配布物の検査に失敗: --version が「$GOT」（期待は「mrcr $VERSION」）" >&2
  rm -f "$ZIP"
  exit 1
fi

echo "==> Build successful: $ZIP" >&2
echo "$ZIP"
shasum -a 256 "$ZIP" | cut -d' ' -f1
