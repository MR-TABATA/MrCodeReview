// レビューが見た差分を、指摘を重ねて表示するための、画面に依存しない部分。
//
// 差分は、保存済みのプロンプトに入っている「その時点の差分」を使う（押したあとにファイルが変わっても、
// 見ているのは指摘されたときの差分）。ハンクごとに旧側・新側のテキストを組み立てて、MRDown の
// `sideBySide` に通す。
//
// 指摘の位置は、モデルが返した行番号ではなく、引用した行の文字列で差分の行を探して決める。
// 行番号は当てにならないが、引用は機械的に確かめられる（Rust 側の「根拠あり」と同じ考え方）。

import { sideBySide, type DiffRow } from './diff';
import type { Finding } from './types';

const DIFF_MARK = '\n## 差分\n\n```diff\n';

/** プロンプトの全文から、送った差分の本文を取り出す。見つからなければ null。 */
export function extractDiff(prompt: string): string | null {
  // アプリが最後に足した差分のブロックを探す。差分の中身に同じ並びが出ても、行頭に +/-/空白が付くので一致しない。
  const at = prompt.lastIndexOf(DIFF_MARK);
  if (at < 0) return null;
  const start = at + DIFF_MARK.length;
  const end = prompt.endsWith('```\n') ? prompt.length - 4 : prompt.length;
  return end >= start ? prompt.slice(start, end) : null;
}

export interface FileDiff {
  path: string;
  /** その差分の本文（`diff --git` の行から） */
  text: string;
}

/** `diff --git` ごとに分ける（Rust の `split_diff` と同じ規則）。 */
export function splitFiles(diff: string): FileDiff[] {
  const out: FileDiff[] = [];
  let cur: { path: string; lines: string[] } | null = null;
  for (const line of diff.split(/(?<=\n)/)) {
    if (line.startsWith('diff --git ')) {
      if (cur) out.push({ path: cur.path, text: cur.lines.join('') });
      const head = line.replace(/\n$/, '');
      const i = head.lastIndexOf(' b/');
      cur = { path: i >= 0 ? head.slice(i + 3) : head, lines: [] };
    }
    cur?.lines.push(line);
  }
  if (cur) out.push({ path: cur.path, text: cur.lines.join('') });
  return out;
}

export interface Hunk {
  /** `@@ -a,b +c,d @@ ...` の行 */
  header: string;
  rows: DiffRow[];
}

export interface ParsedFile {
  path: string;
  added: number;
  removed: number;
  isNew: boolean;
  hunks: Hunk[];
}

const HUNK_RE = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@/;

/** 1 ファイルの差分を、ハンクごとの左右並びの行にする。行番号は、ファイル全体での番号。 */
export function parseFile(file: FileDiff): ParsedFile {
  const parsed: ParsedFile = { path: file.path, added: 0, removed: 0, isNew: false, hunks: [] };
  let cur: { header: string; oldStart: number; newStart: number; old: string[]; neu: string[] } | null = null;

  const flush = () => {
    if (!cur) return;
    const rows = sideBySide(cur.old.join('\n'), cur.neu.join('\n')).map((r) => ({
      ...r,
      leftNo: r.leftNo === null ? null : r.leftNo + cur!.oldStart - 1,
      rightNo: r.rightNo === null ? null : r.rightNo + cur!.newStart - 1,
    }));
    parsed.hunks.push({ header: cur.header, rows });
    cur = null;
  };

  for (const line of file.text.split('\n')) {
    const m = HUNK_RE.exec(line);
    if (m) {
      flush();
      cur = { header: line, oldStart: Number(m[1]), newStart: Number(m[2]), old: [], neu: [] };
      continue;
    }
    if (!cur) {
      if (line.startsWith('new file mode')) parsed.isNew = true;
      continue;
    }
    if (line.startsWith('\\')) continue; // "\ No newline at end of file"
    const mark = line[0];
    const body = line.slice(1);
    if (mark === '+') {
      cur.neu.push(body);
      parsed.added++;
    } else if (mark === '-') {
      cur.old.push(body);
      parsed.removed++;
    } else if (mark === ' ') {
      cur.old.push(body);
      cur.neu.push(body);
    }
    // それ以外（末尾の空行など）は無視する
  }
  flush();
  return parsed;
}

// ── 指摘の位置 ────────────────────────────────────────────────────────────────

const norm = (s: string): string => s.split(/\s+/).filter(Boolean).join(' ');

/** 引用の先頭の空でない行。diff の行頭記号（+/-）が付いていたら外す。 */
function firstEvidenceLine(evidence: string): string {
  for (const l of evidence.split('\n')) {
    const t = norm(l.replace(/^[+-]/, ''));
    if (t) return t;
  }
  return '';
}

function normPath(p: string): string {
  return p.replace(/^\.\//, '').replace(/^a\//, '').replace(/^b\//, '');
}

function sameFile(findingPath: string, path: string): boolean {
  const f = normPath(findingPath);
  return f !== '' && (f === path || path.endsWith('/' + f) || f.endsWith('/' + path));
}

export interface RowRef {
  hunk: number;
  row: number;
  /** 引用が見つかったのは新側（右）か旧側（左）か */
  side: 'new' | 'old';
}

/**
 * 指摘を、差分の行に結びつける。
 * 1. 引用の文字列を含む行（新側を優先）。複数あれば、モデルの行番号に一番近いもの。
 * 2. 引用で見つからなければ、モデルの行番号に一致する新側の行。
 * どちらでも見つからなければ null（位置を特定できない指摘）。
 */
export function locate(file: ParsedFile, finding: Finding): RowRef | null {
  const ev = firstEvidenceLine(finding.evidence);
  const hits: (RowRef & { no: number | null })[] = [];
  if (ev) {
    file.hunks.forEach((h, hi) =>
      h.rows.forEach((r, ri) => {
        if (r.right !== null && norm(r.right).includes(ev)) hits.push({ hunk: hi, row: ri, side: 'new', no: r.rightNo });
      }),
    );
    if (!hits.length) {
      file.hunks.forEach((h, hi) =>
        h.rows.forEach((r, ri) => {
          if (r.left !== null && norm(r.left).includes(ev)) hits.push({ hunk: hi, row: ri, side: 'old', no: r.leftNo });
        }),
      );
    }
  }
  if (hits.length) {
    const want = finding.line;
    const best =
      want === null
        ? hits[0]
        : hits.reduce((a, b) => (Math.abs((b.no ?? 1e9) - want) < Math.abs((a.no ?? 1e9) - want) ? b : a));
    return { hunk: best.hunk, row: best.row, side: best.side };
  }
  if (finding.line !== null) {
    for (let hi = 0; hi < file.hunks.length; hi++) {
      const ri = file.hunks[hi].rows.findIndex((r) => r.rightNo === finding.line);
      if (ri >= 0) return { hunk: hi, row: ri, side: 'new' };
    }
  }
  return null;
}

export interface Placed {
  /** `review.findings` の何番目か */
  index: number;
  at: RowRef | null;
}

export interface FileView {
  file: ParsedFile;
  placed: Placed[];
}

export interface DiffView {
  files: FileView[];
  /** 差分のどのファイルにも結びつかなかった指摘（ファイル名が違う・差分に無いファイル） */
  orphans: number[];
}

/** 差分の全体と、指摘の置き場所をまとめる。差分が取り出せなければ null。 */
export function buildView(prompt: string, findings: Finding[]): DiffView | null {
  const diff = extractDiff(prompt);
  return diff === null ? null : buildViewFromDiff(diff, findings);
}

/** 差分の本文から、直接、組み立てる（「いまの差分」。指摘を重ねないときは、空の配列を渡す）。 */
export function buildViewFromDiff(diff: string, findings: Finding[]): DiffView {
  const files = splitFiles(diff).map(parseFile);
  const views: FileView[] = files.map((file) => ({ file, placed: [] }));
  const orphans: number[] = [];
  findings.forEach((f, index) => {
    const cands = views.filter((x) => sameFile(f.file, x.file.path));
    if (!cands.length) {
      orphans.push(index);
      return;
    }
    // `utils.ts` のように、同名のファイルが複数ありうるときは、
    // パスが完全に一致するもの → 引用の位置が見つかるもの → 最初のもの、の順に選ぶ
    const located = cands.map((v) => ({ v, at: locate(v.file, f) }));
    const chosen =
      located.find((c) => c.v.file.path === normPath(f.file)) ?? located.find((c) => c.at) ?? located[0];
    chosen.v.placed.push({ index, at: chosen.at });
  });
  return { files: views, orphans };
}
