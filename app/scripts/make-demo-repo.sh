#!/usr/bin/env bash
# 試験用の架空リポジトリを .dev/demo-repo に作る（実在のコードは入れない。何度でも作り直せる）。
# - db.js: SQL インジェクションになる変更（未コミット）
# - config.js: 直書きの（偽の）鍵（未コミット）
# - package-lock.json: ロックファイルの変更（送らない対象の見本）
# - notes.txt: 未追跡の新規ファイル（差分に含まれる）
# - app.py: 未使用の import を足した Python（AI のレビューで、差分の小さな変更を見つけられるかを試す見本。linter のボタンは、もう無い）
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIR="$ROOT/.dev/demo-repo"
rm -rf "$DIR"
mkdir -p "$DIR"
cd "$DIR"

g() { git -c user.name=demo -c user.email=demo@example.com -c commit.gpgsign=false "$@"; }

g init -q -b main

cat > db.js <<'EOF'
import { pool } from './pool.js';

export async function findUser(id) {
  return pool.query('SELECT * FROM users WHERE id = $1', [id]);
}
EOF
cat > config.js <<'EOF'
export const config = {
  region: 'ap-northeast-1',
};
EOF
cat > package-lock.json <<'EOF'
{ "name": "demo", "lockfileVersion": 3, "packages": {} }
EOF
cat > app.py <<'EOF'
import re
import sys


def main():
    return sys.argv
EOF
g add .
g commit -q -m "init"

# ここから未コミットの変更
cat > db.js <<'EOF'
import { pool } from './pool.js';

export async function findUser(id) {
  return pool.query("SELECT * FROM users WHERE id = '" + id + "'");
}

export async function findByName(name) {
  return pool.query(`SELECT * FROM users WHERE name = '${name}'`);
}
EOF
cat > config.js <<'EOF'
export const config = {
  region: 'ap-northeast-1',
  apiKey: 'sk-demo-NOT-A-REAL-KEY-0000000000',
};
EOF
cat > package-lock.json <<'EOF'
{ "name": "demo", "lockfileVersion": 3, "packages": { "node_modules/x": { "version": "1.0.0" } } }
EOF
cat > app.py <<'EOF'
import re
import sys
import os


def main():
    return sys.argv
EOF
echo "メモ" > notes.txt

echo "$DIR"
