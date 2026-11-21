#!/usr/bin/env node
// mrcr の起動役。プラットフォーム別パッケージに入っているバイナリを、そのまま起動する。
//
// 守ること:
// - **標準出力に何も書かない。** `mrcr mcp` は標準入出力の JSON-RPC なので、1 バイトでも足すと壊れる。案内は標準エラーへ。
// - 標準入出力はそのまま渡す（stdio: 'inherit'）。終了コードとシグナルを中継する。
// - インストール時に、npm 以外へ通信しない（postinstall は持たない）。
'use strict';

const { spawn } = require('node:child_process');
const path = require('node:path');

// 対応するプラットフォーム別パッケージ。増やすときは、ここと optionalDependencies に足す。
const PACKAGES = {
  darwin: '@mr-tabata/mrcr-darwin',
};

function fail(message) {
  process.stderr.write(`mrcr: ${message}\n`);
  process.exit(1);
}

const name = PACKAGES[process.platform];
if (!name) {
  fail(
    `この OS（${process.platform}）には、まだ対応していません（対応: macOS）。\n` +
      'Releases: https://github.com/MR-TABATA/MrCodeReview/releases'
  );
}

let binary;
try {
  binary = path.join(path.dirname(require.resolve(`${name}/package.json`)), 'bin', 'mrcr');
} catch {
  fail(
    `実行ファイルのパッケージ（${name}）が見つかりません。\n` +
      '`--no-optional` や `--omit=optional` でインストールしていないか確認してください。\n' +
      'Homebrew でも入れられます: brew install mr-tabata/tap/mrcr'
  );
}

// 設定を書く側（mrcr mcp setup）が、npx 経由だと分かるようにする（実行ファイルは npx のキャッシュの中にあり、あとで消えうる）
const child = spawn(binary, process.argv.slice(2), {
  stdio: 'inherit',
  env: { ...process.env, MRCR_VIA_NPX: '1' },
});

for (const sig of ['SIGINT', 'SIGTERM', 'SIGHUP']) {
  process.on(sig, () => {
    if (!child.killed) child.kill(sig);
  });
}

child.on('error', (e) => fail(`起動できませんでした（${binary}）: ${e.message}`));
child.on('exit', (code, signal) => {
  if (signal) {
    // 子がシグナルで終わったときは、自分も同じシグナルで終わる（終了コードの規約）
    process.removeAllListeners(signal);
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 1);
});
