import { describe, it, expect } from 'vitest';
import { buildView, buildViewFromDiff, extractDiff, locate, parseFile, splitFiles } from './diffview';
import type { Finding } from './types';

const DB_DIFF = `diff --git a/db.js b/db.js
index 1111111..2222222 100644
--- a/db.js
+++ b/db.js
@@ -10,3 +10,4 @@ export function x() {
 function find(id) {
-  return null;
+  const q = "SELECT * FROM u WHERE id=" + id;
+  return db.query(q);
 }
`;

const NEW_DIFF = `diff --git a/dir with space/new.js b/dir with space/new.js
new file mode 100644
index 0000000..3333333
--- /dev/null
+++ b/dir with space/new.js
@@ -0,0 +1,2 @@
+danger();
+danger();
\\ No newline at end of file
`;

function promptFor(diff: string): string {
  // Rust の recipe::compose と同じ形
  return `# セキュリティ\n観点\n\n## 出力形式\n\n...\n\n## 差分\n\n\`\`\`diff\n${diff}\`\`\`\n`;
}

function finding(over: Partial<Finding>): Finding {
  return { file: 'db.js', line: null, severity: 'high', title: 't', detail: '', evidence: '', verified: true, rule: null, ...over };
}

describe('extractDiff', () => {
  it('取り出した差分は、送った差分と一致する', () => {
    expect(extractDiff(promptFor(DB_DIFF))).toBe(DB_DIFF);
  });

  it('差分の中に ```diff があっても（markdown の差分）、最後のブロックを取る', () => {
    const md = `diff --git a/a.md b/a.md\n--- a/a.md\n+++ b/a.md\n@@ -1 +1,3 @@\n x\n+\`\`\`diff\n+\`\`\`\n`;
    expect(extractDiff(promptFor(md))).toBe(md);
  });

  it('差分のブロックが無ければ null', () => {
    expect(extractDiff('ただの文章')).toBeNull();
  });
});

describe('splitFiles', () => {
  it('ファイルごとに分け、空白入りのパスも取れ、つなぐと元に戻る', () => {
    const all = DB_DIFF + NEW_DIFF;
    const files = splitFiles(all);
    expect(files.map((f) => f.path)).toEqual(['db.js', 'dir with space/new.js']);
    expect(files.map((f) => f.text).join('')).toBe(all);
    expect(splitFiles('')).toEqual([]);
  });
});

describe('parseFile', () => {
  it('行番号はファイル全体での番号になり、変更行数を数える', () => {
    const p = parseFile(splitFiles(DB_DIFF)[0]);
    expect(p.hunks).toHaveLength(1);
    expect(p.hunks[0].header).toContain('@@ -10,3 +10,4 @@');
    expect(p.added).toBe(2);
    expect(p.removed).toBe(1);
    const rows = p.hunks[0].rows;
    expect(rows.map((r) => [r.type, r.leftNo, r.rightNo])).toEqual([
      ['eq', 10, 10],
      ['mod', 11, 11],
      ['ins', null, 12],
      ['eq', 12, 13],
    ]);
  });

  it('新規ファイルを判定し、"No newline" の行は無視する', () => {
    const p = parseFile(splitFiles(NEW_DIFF)[0]);
    expect(p.isNew).toBe(true);
    expect(p.added).toBe(2);
    expect(p.hunks[0].rows.map((r) => [r.type, r.rightNo])).toEqual([
      ['ins', 1],
      ['ins', 2],
    ]);
  });

  it('複数のハンクを別々に扱う', () => {
    const two = `diff --git a/a.js b/a.js\n--- a/a.js\n+++ b/a.js\n@@ -1,2 +1,2 @@\n a\n-b\n+B\n@@ -50,2 +50,2 @@\n x\n-y\n+Y\n`;
    const p = parseFile(splitFiles(two)[0]);
    expect(p.hunks).toHaveLength(2);
    expect(p.hunks[1].rows.find((r) => r.type === 'mod')?.rightNo).toBe(51);
  });
});

describe('locate', () => {
  const p = parseFile(splitFiles(DB_DIFF)[0]);

  it('引用の文字列で、新側の行を見つける（行番号が間違っていても）', () => {
    const at = locate(p, finding({ evidence: 'const q = "SELECT * FROM u WHERE id=" + id;', line: 99 }));
    expect(at).toEqual({ hunk: 0, row: 1, side: 'new' });
  });

  it('引用に diff の行頭記号や余分な空白が付いていても見つかる', () => {
    expect(locate(p, finding({ evidence: '+  return   db.query(q);' }))).toEqual({ hunk: 0, row: 2, side: 'new' });
  });

  it('引用が複数行なら、先頭の空でない行で探す', () => {
    expect(locate(p, finding({ evidence: '\n  return db.query(q);\nfoo' }))).toEqual({ hunk: 0, row: 2, side: 'new' });
  });

  it('消された行を引用していたら、旧側の行を指す', () => {
    expect(locate(p, finding({ evidence: 'return null;' }))).toEqual({ hunk: 0, row: 1, side: 'old' });
  });

  it('引用が無い・見つからないときは、モデルの行番号（新側）を使う', () => {
    expect(locate(p, finding({ evidence: '', line: 12 }))).toEqual({ hunk: 0, row: 2, side: 'new' });
    expect(locate(p, finding({ evidence: '差分に無い引用', line: 12 }))).toEqual({ hunk: 0, row: 2, side: 'new' });
  });

  it('どちらも合わなければ null', () => {
    expect(locate(p, finding({ evidence: '差分に無い引用', line: 500 }))).toBeNull();
    expect(locate(p, finding({ evidence: '', line: null }))).toBeNull();
  });

  it('同じ引用が複数あるときは、行番号に近いほうを選ぶ', () => {
    const q = parseFile(splitFiles(NEW_DIFF)[0]);
    expect(locate(q, finding({ file: 'dir with space/new.js', evidence: 'danger();', line: 2 }))).toEqual({ hunk: 0, row: 1, side: 'new' });
    expect(locate(q, finding({ file: 'dir with space/new.js', evidence: 'danger();', line: 1 }))).toEqual({ hunk: 0, row: 0, side: 'new' });
    expect(locate(q, finding({ file: 'dir with space/new.js', evidence: 'danger();', line: null }))).toEqual({ hunk: 0, row: 0, side: 'new' });
  });
});

describe('buildView', () => {
  it('指摘をファイルごとに置き、差分に無いファイルの指摘は orphans に出す', () => {
    const findings = [
      finding({ file: 'db.js', evidence: 'return db.query(q);' }),
      finding({ file: 'b/db.js', evidence: '', line: null }), // ファイルは合うが位置は不明
      finding({ file: 'ghost.js', evidence: 'x' }),
    ];
    const v = buildView(promptFor(DB_DIFF + NEW_DIFF), findings)!;
    expect(v.files.map((f) => f.file.path)).toEqual(['db.js', 'dir with space/new.js']);
    expect(v.files[0].placed).toEqual([
      { index: 0, at: { hunk: 0, row: 2, side: 'new' } },
      { index: 1, at: null },
    ]);
    expect(v.files[1].placed).toEqual([]);
    expect(v.orphans).toEqual([2]);
  });

  it('同名のファイルが複数あるときは、引用のあるほうに置く（最初の 1 つに決め打ちしない）', () => {
    const a = 'diff --git a/pkg_a/utils.ts b/pkg_a/utils.ts\n--- a/pkg_a/utils.ts\n+++ b/pkg_a/utils.ts\n@@ -1 +1,2 @@\n x\n+const a = 1;\n';
    const b = 'diff --git a/pkg_b/utils.ts b/pkg_b/utils.ts\n--- a/pkg_b/utils.ts\n+++ b/pkg_b/utils.ts\n@@ -1 +1,2 @@\n y\n+danger(x);\n';
    const v = buildView(promptFor(a + b), [
      finding({ file: 'utils.ts', evidence: 'danger(x);' }), // 引用があるのは pkg_b
      finding({ file: 'pkg_a/utils.ts', evidence: 'danger(x);' }), // パスが完全に一致するほうを優先（位置は無い）
      finding({ file: 'utils.ts', evidence: '', line: null }), // どちらにも結びつかない → 最初のもの
    ])!;
    const [fa, fb] = v.files;
    expect(fb.placed.map((p) => p.index)).toEqual([0]);
    expect(fb.placed[0].at).toEqual({ hunk: 0, row: 1, side: 'new' });
    expect(fa.placed.map((p) => p.index)).toEqual([1, 2]);
    expect(fa.placed[0].at).toBeNull();
    expect(v.orphans).toEqual([]);
  });

  it('差分が取り出せなければ null', () => {
    expect(buildView('差分なし', [])).toBeNull();
  });
});

describe('buildViewFromDiff（「いまの差分」）', () => {
  it('差分の本文から、直接、組み立てる。指摘を重ねない（空の配列）ときは、置き場所も、行き場の無い指摘も、無い', () => {
    const v = buildViewFromDiff(DB_DIFF + NEW_DIFF, []);
    expect(v.files.map((f) => f.file.path)).toEqual(['db.js', 'dir with space/new.js']);
    expect(v.files.every((f) => f.placed.length === 0)).toBe(true);
    expect(v.orphans).toEqual([]);
    expect(v.files[0]?.file.added).toBe(2);
  });
  it('プロンプトから組み立てる buildView と、同じ結果になる（差分の本文が同じなら）', () => {
    const prompt = `前置き\n## 差分\n\n\`\`\`diff\n${DB_DIFF}\`\`\`\n`;
    const a = buildView(prompt, []);
    const b = buildViewFromDiff(extractDiff(prompt)!, []);
    expect(a).toEqual(b);
  });
  it('空の差分でも、落ちない（ファイルなし）', () => {
    expect(buildViewFromDiff('', [])).toEqual({ files: [], orphans: [] });
  });
});
