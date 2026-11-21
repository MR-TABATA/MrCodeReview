// 起動役（bin/mrcr.js）の試験。`node --test npm/mrcr/test` で動く。npm には公開しない。
// ダミーの「プラットフォーム別パッケージ」を、一時の場所に組み立てて、本物の起動役をそこへ置いて動かす。
'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawn, spawnSync } = require('node:child_process');

const LAUNCHER = path.join(__dirname, '..', 'bin', 'mrcr.js');
const ON_MAC = process.platform === 'darwin';

// dir/bin/mrcr.js（起動役）と、dir/node_modules/@mr-tabata/mrcr-darwin/bin/mrcr（ダミー。script の中身）を作る
function setup(script) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'mrcr-launcher-'));
  fs.mkdirSync(path.join(dir, 'bin'), { recursive: true });
  fs.copyFileSync(LAUNCHER, path.join(dir, 'bin', 'mrcr.js'));
  if (script !== null) {
    const pkg = path.join(dir, 'node_modules', '@mr-tabata', 'mrcr-darwin');
    fs.mkdirSync(path.join(pkg, 'bin'), { recursive: true });
    fs.writeFileSync(path.join(pkg, 'package.json'), '{"name":"@mr-tabata/mrcr-darwin","version":"0.0.0"}');
    fs.writeFileSync(path.join(pkg, 'bin', 'mrcr'), script, { mode: 0o755 });
  }
  return { dir, launcher: path.join(dir, 'bin', 'mrcr.js'), clean: () => fs.rmSync(dir, { recursive: true, force: true }) };
}

test('子の終了コードが、そのまま終了コードになる', { skip: !ON_MAC }, () => {
  const t = setup('#!/bin/sh\nexit 3\n');
  try {
    const r = spawnSync(process.execPath, [t.launcher, 'x'], { encoding: 'utf8' });
    assert.equal(r.status, 3);
  } finally { t.clean(); }
});

test('引数・標準入出力が、そのまま渡る（標準出力に起動役が足さない）', { skip: !ON_MAC }, () => {
  const t = setup('#!/bin/sh\necho "args:$*"\ncat\n');
  try {
    const r = spawnSync(process.execPath, [t.launcher, 'mcp', '--flag'], { input: 'hello\n', encoding: 'utf8' });
    assert.equal(r.status, 0);
    assert.equal(r.stdout, 'args:mcp --flag\nhello\n', '標準出力は、子の出力だけ');
    assert.equal(r.stderr, '');
  } finally { t.clean(); }
});

test('環境変数 MRCR_VIA_NPX=1 が、子に付く', { skip: !ON_MAC }, () => {
  const t = setup('#!/bin/sh\necho "via=$MRCR_VIA_NPX"\n');
  try {
    const r = spawnSync(process.execPath, [t.launcher], { encoding: 'utf8' });
    assert.equal(r.stdout, 'via=1\n');
  } finally { t.clean(); }
});

test('パッケージが無いとき: 標準出力は空、標準エラーに案内、終了コード 1', { skip: !ON_MAC }, () => {
  const t = setup(null);
  try {
    const r = spawnSync(process.execPath, [t.launcher, 'mcp'], { encoding: 'utf8' });
    assert.equal(r.status, 1);
    assert.equal(r.stdout, '', 'MCP の標準出力を壊さない');
    assert.match(r.stderr, /@mr-tabata\/mrcr-darwin/);
  } finally { t.clean(); }
});

test('対応外の OS: 標準出力は空、標準エラーに案内、終了コード 1', () => {
  const t = setup(null);
  const preload = path.join(t.dir, 'fake-platform.js');
  fs.writeFileSync(preload, "Object.defineProperty(process, 'platform', { value: 'linux' });\n");
  try {
    const r = spawnSync(process.execPath, ['--require', preload, t.launcher, 'mcp'], { encoding: 'utf8' });
    assert.equal(r.status, 1);
    assert.equal(r.stdout, '');
    assert.match(r.stderr, /linux/);
  } finally { t.clean(); }
});

test('シグナル: SIGTERM を起動役に送ると、子も終わり、起動役も SIGTERM で終わる', { skip: !ON_MAC }, async () => {
  const marker = path.join(os.tmpdir(), `mrcr-launcher-pid-${process.pid}`);
  const t = setup(`#!/bin/sh\necho $$ > "${marker}"\nexec sleep 30\n`);
  try {
    const child = spawn(process.execPath, [t.launcher], { stdio: 'ignore' });
    const exited = new Promise((resolve) => child.on('exit', (code, signal) => resolve({ code, signal })));

    // 子のバイナリが起動して、pid を書くまで待つ
    for (let i = 0; i < 100 && !fs.existsSync(marker); i++) await new Promise((r) => setTimeout(r, 50));
    const childPid = Number(fs.readFileSync(marker, 'utf8').trim());
    assert.ok(childPid > 0, '子のバイナリが起動した');

    child.kill('SIGTERM');
    const result = await exited;
    assert.equal(result.signal, 'SIGTERM', '子がシグナルで終わったら、起動役も同じシグナルで終わる');

    let alive = true;
    for (let i = 0; i < 40 && alive; i++) {
      try { process.kill(childPid, 0); await new Promise((r) => setTimeout(r, 50)); } catch { alive = false; }
    }
    assert.equal(alive, false, '子のプロセスが残っていない');
  } finally { t.clean(); fs.rmSync(marker, { force: true }); }
});
