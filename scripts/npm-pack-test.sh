#!/bin/sh
# scripts/npm-pack.sh の試験。npm には公開しない。一時の場所（mktemp）で組み立てて、終わったら消す。
#
#     sh scripts/npm-pack-test.sh
#
# 見ること:
#   1. 版が Cargo.toml と違えば、終了コード 1 で止まり、tgz を作らない
#   2. 版が一致すれば、2 つの tgz ができ、本体の optionalDependencies の版がその版になっている
#   3. 出力先に、前の（古い）tgz があっても、作り直される
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
printf '#!/bin/sh\necho dummy\n' > "$WORK/mrcr-bin"

REAL=$(grep '^version = ' crates/mrcr/Cargo.toml | head -n 1 | sed 's/version = "\(.*\)"/\1/')
FAIL=0
ng() { echo "NG: $1" >&2; FAIL=1; }

# 1. 版の不一致
if sh scripts/npm-pack.sh 9.9.9 "$WORK/mrcr-bin" "$WORK/out-bad" >/dev/null 2>&1; then
  ng "版が違うのに、成功した"
fi
if ls "$WORK"/out-bad/*.tgz >/dev/null 2>&1; then
  ng "版が違うのに、tgz ができた"
fi

# 2. 版の一致
OUT="$WORK/out-ok"
if ! sh scripts/npm-pack.sh "$REAL" "$WORK/mrcr-bin" "$OUT" >/dev/null 2>&1; then
  ng "版が一致するのに、失敗した"
fi
[ -f "$OUT/mrcr-$REAL.tgz" ] || ng "本体の tgz が無い"
[ -f "$OUT/mr-tabata-mrcr-darwin-$REAL.tgz" ] || ng "プラットフォーム別の tgz が無い"

DEP=$(tar -xzOf "$OUT/mrcr-$REAL.tgz" package/package.json | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>{const j=JSON.parse(s);console.log(j.version+" "+j.optionalDependencies["@mr-tabata/mrcr-darwin"])})')
[ "$DEP" = "$REAL $REAL" ] || ng "本体の版と optionalDependencies の版が ${REAL} ではない（${DEP}）"

PLAT=$(tar -xzOf "$OUT/mr-tabata-mrcr-darwin-$REAL.tgz" package/package.json | node -e 'let s="";process.stdin.on("data",d=>s+=d).on("end",()=>console.log(JSON.parse(s).version))')
[ "$PLAT" = "$REAL" ] || ng "プラットフォーム別の版が ${REAL} ではない（${PLAT}）"

# 3. 古い tgz があっても作り直される（中身が dummy と同じ新しいものになる）
echo stale > "$OUT/mrcr-$REAL.tgz"
sh scripts/npm-pack.sh "$REAL" "$WORK/mrcr-bin" "$OUT" >/dev/null 2>&1 || ng "再実行に失敗した"
tar -tzf "$OUT/mrcr-$REAL.tgz" >/dev/null 2>&1 || ng "古い tgz が作り直されていない"

if [ "$FAIL" -eq 0 ]; then echo "OK: npm-pack.sh の試験（版 ${REAL}）"; else exit 1; fi
