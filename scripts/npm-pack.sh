#!/bin/sh
# npm パッケージ（tarball）を作る。公開はしない。
#
#     sh scripts/npm-pack.sh 1.11.0 path/to/mrcr [出力先]
#
# 第 2 引数: universal binary（release.sh の zip を解凍したもの）。出力先の既定は dist/npm。
# 出力: <出力先>/mr-tabata-mrcr-<版>.tgz（本体 @mr-tabata/mrcr）と <出力先>/mr-tabata-mrcr-darwin-<版>.tgz
# 版は crates/mrcr/Cargo.toml と一致していなければ止まる（release.sh と同じ規則）。
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

VERSION="${1:-}"
BIN="${2:-}"
OUT="${3:-dist/npm}"
if [ -z "$VERSION" ] || [ -z "$BIN" ]; then
  echo "使い方: sh scripts/npm-pack.sh <版> <mrcr のバイナリ> [出力先]" >&2
  exit 1
fi
[ -f "$BIN" ] || { echo "バイナリが無い: $BIN" >&2; exit 1; }

CARGO_VER=$(grep '^version = ' crates/mrcr/Cargo.toml | head -n 1 | sed 's/version = "\(.*\)"/\1/')
if [ "$CARGO_VER" != "$VERSION" ]; then
  echo "crates/mrcr/Cargo.toml は ${CARGO_VER}、配る版は ${VERSION}。先に Cargo.toml を直すこと" >&2
  exit 1
fi

# バイナリが、この版の、universal（arm64 と x86_64）であること（release.sh と同じ規則）。
# 違うものが、別の版・片方の CPU だけのまま公開されると、setup が書く版固定の npx 形式とずれ、片方の Mac で動かない。
GOT="$("$BIN" --version 2>/dev/null || true)"
if [ "$GOT" != "mrcr $VERSION" ]; then
  echo "バイナリの版が違う: --version が「$GOT」（期待は「mrcr $VERSION」）: $BIN" >&2
  exit 1
fi
ARCHS=" $(lipo -archs "$BIN" 2>/dev/null || true) "
for A in arm64 x86_64; do
  case "$ARCHS" in
    *" $A "*) ;;
    *) echo "バイナリに $A が無い（universal ではない）: $BIN（${ARCHS}）" >&2; exit 1 ;;
  esac
done

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE/mrcr/bin" "$STAGE/mrcr-darwin/bin"

cp npm/mrcr/bin/mrcr.js "$STAGE/mrcr/bin/mrcr.js"
cp "$BIN" "$STAGE/mrcr-darwin/bin/mrcr"
cp npm/mrcr/README.md "$STAGE/mrcr/README.md"
cp LICENSE "$STAGE/mrcr/LICENSE"
cp LICENSE "$STAGE/mrcr-darwin/LICENSE"
chmod +x "$STAGE/mrcr/bin/mrcr.js" "$STAGE/mrcr-darwin/bin/mrcr"

# 版を、2 つの package.json（と、本体の optionalDependencies）に入れる
node -e '
const fs = require("fs");
const v = process.argv[1];
for (const [src, dst, fix] of [
  ["npm/mrcr/package.json", process.argv[2] + "/mrcr/package.json", (j) => { j.optionalDependencies["@mr-tabata/mrcr-darwin"] = v; }],
  ["npm/mrcr-darwin/package.json", process.argv[2] + "/mrcr-darwin/package.json", () => {}],
]) {
  const j = JSON.parse(fs.readFileSync(src, "utf8"));
  j.version = v;
  delete j.private; // 元の package.json は、誤って公開されないよう private。公開用に組み立てるときだけ外す
  fix(j);
  fs.writeFileSync(dst, JSON.stringify(j, null, 2) + "\n");
}
' "$VERSION" "$STAGE"

mkdir -p "$OUT"
OUT_ABS="$(cd "$OUT" && pwd)"

# npm pack は、作ったファイル名を標準出力に出す。エラーと警告は標準エラーに出るので、捨てない。
# 失敗したら set -e で止まる。古い tgz を成功と取り違えないよう、先に、この版の分を消してから作り、npm が言ったファイルの存在を確かめる。
pack() {
  NAME=$(cd "$1" && npm pack --pack-destination "$OUT_ABS" | tail -n 1)
  [ -n "$NAME" ] && [ -f "$OUT_ABS/$NAME" ] || { echo "npm pack の結果のファイルが無い: $1（出力: ${NAME:-なし}）" >&2; exit 1; }
  echo "$OUT_ABS/$NAME" >&2
}

rm -f "$OUT_ABS"/mr-tabata-mrcr-"$VERSION".tgz "$OUT_ABS"/mr-tabata-mrcr-darwin-"$VERSION".tgz
pack "$STAGE/mrcr-darwin"
pack "$STAGE/mrcr"
