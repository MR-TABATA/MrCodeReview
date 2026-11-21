#!/bin/sh
# scripts/npm-pack.sh の試験。npm には公開しない。一時の場所（mktemp）で組み立てて、終わったら消す。
#
#     sh scripts/npm-pack-test.sh
#
# ダミーのバイナリは、cc で作る（macOS の clang。本物の universal（arm64 + x86_64）にするため）。
# 見ること:
#   1. 版が Cargo.toml と違えば、終了コード 1 で止まり、tgz を作らない
#   2. バイナリの版が違う・universal でない（arm64 だけ）ときも、止まり、tgz を作らない
#   3. 一致すれば、2 つの tgz ができ、版・optionalDependencies の版が一致し、private が外れ、LICENSE・README が入る
#   4. 出力先に古い tgz があっても、作り直される
#   5. 元の package.json は private（誤って、そのまま公開できない）
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
REAL=$(grep '^version = ' crates/mrcr/Cargo.toml | head -n 1 | sed 's/version = "\(.*\)"/\1/')
FAIL=0
ng() { echo "NG: $1" >&2; FAIL=1; }

# make_bin <出力> <表示する版> <cc の -arch 指定...>
make_bin() {
  OUTBIN="$1"; SHOWN="$2"; shift 2
  # ソースはファイルに書く（標準入力だと、複数の -arch の 2 つ目に渡らない）
  printf '#include <stdio.h>\nint main(void){puts("mrcr %s");return 0;}\n' "$SHOWN" > "$OUTBIN.c"
  cc "$@" "$OUTBIN.c" -o "$OUTBIN"
}
make_bin "$WORK/good" "$REAL" -arch arm64 -arch x86_64
make_bin "$WORK/wrongver" "0.0.1" -arch arm64 -arch x86_64
make_bin "$WORK/armonly" "$REAL" -arch arm64

# 1. 版の不一致（引数と Cargo.toml）
if sh scripts/npm-pack.sh 9.9.9 "$WORK/good" "$WORK/out-ver" >/dev/null 2>&1; then ng "版が違うのに、成功した"; fi
ls "$WORK"/out-ver/*.tgz >/dev/null 2>&1 && ng "版が違うのに、tgz ができた"

# 2. バイナリの不一致
if sh scripts/npm-pack.sh "$REAL" "$WORK/wrongver" "$WORK/out-wv" >/dev/null 2>&1; then ng "バイナリの版が違うのに、成功した"; fi
ls "$WORK"/out-wv/*.tgz >/dev/null 2>&1 && ng "バイナリの版が違うのに、tgz ができた"
if sh scripts/npm-pack.sh "$REAL" "$WORK/armonly" "$WORK/out-arm" >/dev/null 2>&1; then ng "arm64 だけのバイナリで、成功した"; fi
ls "$WORK"/out-arm/*.tgz >/dev/null 2>&1 && ng "arm64 だけのバイナリで、tgz ができた"

# 3. 一致
OUT="$WORK/out-ok"
sh scripts/npm-pack.sh "$REAL" "$WORK/good" "$OUT" >/dev/null 2>&1 || ng "一致するのに、失敗した"
MAIN="$OUT/mr-tabata-mrcr-$REAL.tgz"; PLAT="$OUT/mr-tabata-mrcr-darwin-$REAL.tgz"
[ -f "$MAIN" ] || ng "本体の tgz が無い"
[ -f "$PLAT" ] || ng "プラットフォーム別の tgz が無い"

field() { # <tgz> <js の式（j が package.json）>
  tar -xzOf "$1" package/package.json | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const j=JSON.parse(s);console.log('"$2"')})'
}
[ "$(field "$MAIN" 'j.version+" "+j.optionalDependencies["@mr-tabata/mrcr-darwin"]')" = "$REAL $REAL" ] || ng "本体の版と optionalDependencies の版が ${REAL} ではない"
[ "$(field "$PLAT" 'j.version')" = "$REAL" ] || ng "プラットフォーム別の版が ${REAL} ではない"
[ "$(field "$MAIN" 'j.private')" = "undefined" ] || ng "本体に private が残っている（公開できない）"
[ "$(field "$PLAT" 'j.private')" = "undefined" ] || ng "プラットフォーム別に private が残っている（公開できない）"
for T in "$MAIN" "$PLAT"; do
  tar -tzf "$T" | grep -q '^package/LICENSE$' || ng "$T に LICENSE が無い"
done
tar -tzf "$MAIN" | grep -q '^package/README.md$' || ng "本体に README.md が無い"
# 組み立てたバイナリが、そのまま動く
tar -xzOf "$PLAT" package/bin/mrcr > "$WORK/extracted" && chmod +x "$WORK/extracted"
[ "$("$WORK/extracted" --version)" = "mrcr $REAL" ] || ng "tgz の中のバイナリが動かない"

# 4. 古い tgz があっても作り直される
echo stale > "$MAIN"
sh scripts/npm-pack.sh "$REAL" "$WORK/good" "$OUT" >/dev/null 2>&1 || ng "再実行に失敗した"
tar -tzf "$MAIN" >/dev/null 2>&1 || ng "古い tgz が作り直されていない"

# 5. 元の package.json は private
for P in npm/mrcr/package.json npm/mrcr-darwin/package.json; do
  [ "$(node -e 'console.log(JSON.parse(require("fs").readFileSync(process.argv[1],"utf8")).private)' "$P")" = "true" ] || ng "$P が private ではない"
done

if [ "$FAIL" -eq 0 ]; then echo "OK: npm-pack.sh の試験（版 ${REAL}）"; else exit 1; fi
