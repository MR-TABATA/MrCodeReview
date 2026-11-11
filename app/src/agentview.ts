// エージェントの「対応の報告」を、会話の記録（ログ）から取り込む画面の部品。
//
// - 一覧の上の細い 1 行の右端に、「対応内容を取り込む」（Claude Code・agy・Copilot CLI のログを読む）。初めては、ログを読む許可を、1 回だけ聞く（読んでよいのは、
//   識別子を含む行だけ。会話の本文は、保存しない）。
// - 取り込んだ報告は、各指摘のカードに、「エージェントの申告」として出す。**実際に直ったかの確認ではない**ことを、必ず添える。
// - 見つからなかったときは、推定で埋めず、調べた数を言う（何も無いのと、読めなかったのを、取り違えない）。

import { h } from './dom';
import { getAppLang, msg } from './i18n';
import type { AgentFetch, AgentReport, Note } from './types';

// ── 申告の状態（「対応にした指摘ごとに」、申告が保存されているか） ──
//
// 申告は、2 つの経路で、同じ表に入る: ① 会話の記録（ログ）から取り込む（このボタン）。② エージェントが、`mrcr handoff report` で、直接保存する。
// ②で保存された分は、取り込みを押さなくても、すでに DB にある。**取り込みのボタンは、申告が足りない指摘があるときだけ**出す
// （一部だけ報告された、という抜け・漏れのときに、残りを取り込めるように、レビュー単位ではなく、指摘ごとに見る）。

export interface AgentStatus {
  /** 報告を求める指摘の数: 「対応」で、あなたが「対応済み」にしていないもの */
  total: number;
  /** そのうち、申告が保存されているもの（「対応した」も「対応しなかった」も、報告済みに数える） */
  reported: number;
  /** 申告が足りないもの */
  missing: number;
  /**
   * 報告済みのうち、「**対応しなかった**」と申告されたもの。エージェントが、直さないと決めた指摘で、**あなたの判断が、まだ残っている**
   * （理由を読んで、「無視」「待ち」にするか、もう一度、直す対象にするか）。「保存済み」と言うだけでは、完了したように見えるので、別に数える。
   */
  skipped: number;
}

export function agentStatus(
  triage: readonly string[],
  notes: readonly (Note | null | undefined)[] | undefined,
  reports: readonly (AgentReport | null | undefined)[] | undefined,
): AgentStatus {
  let total = 0;
  let reported = 0;
  let skipped = 0;
  triage.forEach((state, i) => {
    if (state !== 'fix') return;
    if ((notes?.[i]?.done_at ?? null) !== null) return; // あなたが「対応済み」にしたものは、報告を求めない
    total += 1;
    const report = reports?.[i];
    if (report) reported += 1;
    if (report?.verdict === 'skipped') skipped += 1;
  });
  return { total, reported, missing: total - reported, skipped };
}

export type AgentSource = 'cli' | 'mcp' | 'log';

/** 申告の保存元。`cli:<名前>`（`mrcr handoff report`）・`mcp:<名前>`・それ以外は、会話の記録（ログ）から取り込んだもの。 */
export const agentSourceKind = (source: string): AgentSource => (source.startsWith('cli:') ? 'cli' : source.startsWith('mcp:') ? 'mcp' : 'log');

/** 保存元を、人が読める言葉にする。 */
export function agentSourceLabel(source: string): string {
  switch (agentSourceKind(source)) {
    case 'cli':
      return `${source.slice(4)} が、mrcr handoff report で保存`;
    case 'mcp':
      return `${source.slice(4)} が、MCP で保存`;
    default:
      return 'ログから取り込み';
  }
}

export interface AgentUi {
  busy: boolean;
  /** 許可を聞いている最中（まだ、ログは読んでいない） */
  asking: boolean;
  result: AgentFetch | null;
  error: string;
}

export const newAgentUi = (): AgentUi => ({ busy: false, asking: false, result: null, error: '' });

export interface AgentActions {
  /** `consent` が true のとき、許可を覚えて、ログを読む */
  fetch(consent: boolean): void;
  /** 許可を聞くのをやめる */
  cancel(): void;
}

/** 取り込んだ結果の 1 文。何も見つからなかったときは、調べた数と、考えられる理由を言う。 */
export function agentSummary(r: AgentFetch): string {
  const parts: string[] = [];
  if (r.found > 0) {
    parts.push(`${r.found} 件の報告を取り込みました${r.updated > 0 ? `（${r.updated} 件を、新しく保存・更新）` : '（変更はありません）'}。`);
  } else {
    parts.push(
      `会話の記録からは、報告は見つかりませんでした（調べたログ ${r.files_scanned} 件、識別子を含むログ ${r.files_with_tag} 件）。` +
        'エージェントが、まだ終わっていないか、文面の末部の「最後の報告」を書いていません。または、対応していないエージェントです（読めるのは、Claude Code・agy・Copilot CLI のログだけ）。',
    );
    // すでに保存されている申告（`mrcr handoff report` で保存したものなど）を、無かったことにして見せない
    const stored = (r.agent_reports ?? []).filter(Boolean).length;
    if (stored > 0) parts.push(`すでに保存されている申告 ${stored} 件は、そのままです。`);
  }
  if (r.ignored > 0) parts.push(`このレビューに無い番号の報告が ${r.ignored} 件ありました（取り込んでいません）。`);
  if (r.unreadable_lines > 0) parts.push(`読めなかった行が ${r.unreadable_lines} 件あります（壊れた行・時刻のない行）。`);
  // 「見つからなかった」と「調べていない」を、取り違えない
  if (r.skipped_files > 0) parts.push(`上限を超えたため、古いログ ${r.skipped_files} 件は、調べていません（Claude Code・agy・Copilot CLI それぞれ、新しい順に 30 件まで）。`);
  return parts.join('');
}

/** 一覧の上の細い 1 行の右端に置く、「対応内容を取り込む」と、その結果。 */
export function agentFetchBar(ui: AgentUi, act: AgentActions): HTMLElement {
  const m = msg(getAppLang()).mainUi;
  return h(
    'span',
    { class: 'agentfetch' },
    h(
      'button',
      { class: 'small', disabled: ui.busy, title: m.fetchAgentTitle, onclick: () => act.fetch(false) },
      ui.busy ? m.fetchingAgentBtn : m.fetchAgentBtn,
    ),
  );
}

/**
 * 一覧の上の、申告の状態と、取り込みのボタン。**申告が足りない指摘があるときだけ**、ボタンを出す。
 * - 対応の指摘が 0 件: 何も出さない。
 * - 全部に申告がある: 「対応内容: 保存済み（5/5）」。ボタンは、出さない。
 * - 一部、または、1 件も無い: 状態と、「対応内容を取り込む」。
 */
export function agentStatusBar(status: AgentStatus, ui: AgentUi, act: AgentActions): HTMLElement | null {
  if (status.total === 0) return null;
  const m = msg(getAppLang()).mainUi;
  const base =
    status.missing === 0
      ? m.agentStateAll(status.reported, status.total)
      : status.reported > 0
        ? m.agentStatePartial(status.reported, status.total, status.missing)
        : m.agentStateNone(status.missing);
  // 「対応しなかった」の申告は、あなたの判断が残っている。「保存済み」だけだと、完了したように見えるので、手がかりを添える
  const label = status.skipped > 0 ? `${base}・${m.agentStateDecide(status.skipped)}` : base;
  const kind = status.skipped > 0 ? 'decide' : status.missing === 0 ? 'ok' : 'missing';
  return h(
    'span',
    { class: 'agentfetch' },
    h('span', { class: `agentstate ${kind}` }, label),
    status.missing > 0 ? agentFetchBar(ui, act) : null,
  );
}

/** 取り込みの結果・許可の確認・失敗を、一覧の上に出す枠。何も無ければ null。 */
export function agentNotice(ui: AgentUi, act: AgentActions): HTMLElement | null {
  if (ui.error) return h('p', { class: 'err' }, ui.error);
  if (ui.asking) {
    return h(
      'div',
      { class: 'confirm agentask' },
      h('p', {}, h('strong', {}, '会話の記録（Claude Code・agy・GitHub Copilot CLI のログ）を読んでよいですか？')),
      h('p', {}, '読むのは、このレビューの識別子を含む、エージェントの返事の行だけです。会話の本文は、保存しません。保存するのは、報告の 1 行と、ログの名前・時刻だけです。この許可は、覚えます。'),
      h('div', { class: 'row' }, h('button', { class: 'run', onclick: () => act.fetch(true) }, '読む'), h('button', { onclick: () => act.cancel() }, 'やめる')),
    );
  }
  if (ui.result && !ui.result.needs_consent) return h('p', { class: ui.result.found > 0 ? 'muted agentsum' : 'muted agentsum none' }, agentSummary(ui.result));
  return null;
}

// ── 窓が前面に戻ったときの、申告の読み直し ──
//
// エージェントが `mrcr handoff report` で保存した申告は、**アプリの外**で、DB に入る。開いているレビューは、
// レビューを選んだときにしか、申告を読まないので、窓に戻ってきたとき、申告だけを読み直す（読むだけ。何も書かない）。
// 読んでいる間に、別のレビューへ移っていたら、捨てる。読めなかったときは、いまの表示を消さない。

export interface AgentRefreshDeps {
  /** いま開いているレビュー。無ければ null */
  current: () => { id: string; agent_reports: (AgentReport | null)[] } | null;
  /** 取り込み中・レビュー実行中は、読み直さない */
  busy: () => boolean;
  now: () => number;
  /** 申告を読む（DB から）。レビューが無ければ null */
  load: (id: string) => Promise<(AgentReport | null)[] | null>;
  /** 変わっていたときだけ呼ぶ */
  apply: (id: string, next: (AgentReport | null)[]) => void;
  /** 直前に読んだ時刻（呼び出しの間で、持ち回る） */
  gate: { at: number };
  /** 続けて読まない間隔（ミリ秒）。既定は 2000 */
  minIntervalMs?: number;
}

/** 読み直して、変わっていたら `apply` を呼ぶ。変えたら true。 */
export async function refreshAgentReportsOnFocus(d: AgentRefreshDeps): Promise<boolean> {
  const cur = d.current();
  if (!cur || d.busy()) return false;
  const now = d.now();
  if (now - d.gate.at < (d.minIntervalMs ?? 2000)) return false;
  d.gate.at = now;
  let next: (AgentReport | null)[] | null;
  try {
    next = await d.load(cur.id);
  } catch {
    return false; // 一時的な失敗で、いまの表示を消さない
  }
  const still = d.current();
  if (!next || !still || still.id !== cur.id) return false; // 読んでいる間に、別のレビューへ移った
  if (JSON.stringify(next) === JSON.stringify(still.agent_reports)) return false; // 変わっていない
  d.apply(cur.id, next);
  return true;
}

/**
 * 「対応した」という申告が付いた「対応」の指摘は、終わったものとして、畳む（1 行だけ見せる）。
 * 「対応しなかった」の申告は、畳まない（理由を見て、待ち・無視・もう一度渡す、を決める）。
 * 仕分けを、「対応」から変えたものも、畳まない（使う人の判断を、申告で隠さない）。
 * 畳んだものは、開いても、読むだけにする（仕分け・「CLI に渡す」・メモは、出さない）。やり直すなら、もう一度レビューを実行する。
 */
export const agentDone = (report: AgentReport | null | undefined, status: string): boolean => report?.verdict === 'did' && status === 'fix';

/** 畳んだ指摘の 1 行。押すと、開く（開いたあとは、同じ見出しの行を押すと、閉じる）。`by` は、対応済みにした人（エージェントの申告か、あなたか）。 */
export function foldedRow(f: { severity: string; title: string }, onOpen: () => void, no?: { index: number; tag: string }, by: 'agent' | 'you' = 'agent'): HTMLElement {
  return h(
    'article',
    { class: 'finding folded s-fix' },
    h(
      'button',
      { class: 'foldrow', title: no ? `押すと、指摘と、エージェントの申告を開きます（${no.tag}）` : '押すと、指摘と、エージェントの申告を開きます', onclick: onOpen },
      no ? h('span', { class: 'fc-no', title: `識別子: ${no.tag}` }, `#${no.index + 1}`) : null,
      h('span', { class: `badge sev-${f.severity}` }, f.severity),
      h('strong', {}, f.title || '(見出しなし)'),
      h('span', { class: 'foldmark' }, by === 'you' ? '✓ 対応済み（あなたの申告）' : '✓ 対応済み（エージェントの申告）'),
      h('span', { class: 'foldcaret' }, '▸'),
    ),
  );
}

/** 「対応した」の申告に添える。申告は、エージェントの言葉で、MCR が、コードを確かめたものではない。 */
export const AGENT_DID_NOTE = 'これは、エージェントの申告です。MCR は、コードが実際に直ったかを、確かめていません。';
/** 「対応しなかった」の申告に添える。直していないので、「直ったか」ではなく、「判断が正しいか」を、確かめていない、と言う。 */
export const AGENT_SKIPPED_NOTE = 'これは、エージェントの判断です。MCR は、その判断が正しいかを、確かめていません。読んで、あなたが決めてください。';

/**
 * カードの中の、「エージェントの申告」。報告が無ければ null（「記録なし」を、文字では出さない）。
 * 畳む操作は、ここには置かない（開いた指摘の、見出しの行を押すと閉じる。`main.ts` の `findingCard`）。
 * `onDiff` を渡すと、「いまの差分を見る」を付ける（申告は、言葉にすぎない。実際に、何が変わったかを、差分で見られる）。
 */
export function agentBox(report: AgentReport | null | undefined, when: (ms: number) => string, onDiff?: () => void): HTMLElement | null {
  if (!report) return null;
  const did = report.verdict === 'did';
  return h(
    'div',
    { class: `agentrep ${did ? 'did' : 'skipped'}` },
    h('div', { class: 'row' }, h('span', { class: 'badge' }, did ? '対応した' : '対応しなかった'), h('strong', {}, 'エージェントの申告')),
    h('p', { class: 'agenttext' }, report.text),
    h('div', { class: 'muted' }, `${when(report.reported_at)} ・ ${agentSourceLabel(report.source)}`),
    // 「何の」「誰の」確認ではないのかを、申告の種類ごとに、はっきり言う
    h('div', { class: 'muted' }, did ? AGENT_DID_NOTE : AGENT_SKIPPED_NOTE),
    did ? h('div', { class: 'muted' }, '確かめるには、もう一度レビューを実行してください（次のレビューに、解消したかが出ます）。') : null,
    onDiff ? h('div', {}, h('button', { class: 'small linkish difflink', title: '差分タブの「いま」を開きます（読み取りだけ）', onclick: onDiff }, 'いまの差分を見る')) : null,
  );
}
