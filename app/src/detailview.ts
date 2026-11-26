// 結果の「詳細」タブ。1 回のレビューについて、記録している値を、ぜんぶ並べる（旧「トークン」タブも、ここに含める）。
//
// - 値は、記録した実測値だけ。提供元が報告しなかった項目は、推定で埋めず、「（報告なし）」と言う。
// - 履歴の一覧は、目次に絞った。細かい数字は、ここに集める。あとで、観点別・LLM 別などの統計に育てるときの、元になる項目でもある。

import { agentSourceKind } from './agentview';
import { reviewTag } from './findingtag';
import { h } from './dom';
import { getAppLang, msg, type Lang } from './i18n';
import { ut } from './uitext';
import { providerLabel } from './providertext';
import type { ReviewView } from './types';

/** 画面側の整形（`main.ts` と同じ書き方に揃えるため、渡してもらう） */
export interface DetailFmt {
  when(ms: number): string;
  dur(ms: number): string;
  /** 数。null（報告なし）は、「（報告なし）」 */
  num(n: number | null): string;
  /** 比較元の表示（無ければ空文字） */
  base: string;
}

/** 「根拠あり／根拠不一致」の意味。この言葉が出る場所（詳細タブの指摘の表・カードの引用・「⚠ 根拠不一致」）に、添える。日本語の文言（英語は `ut().detail.groundingNote`） */
export const GROUNDING_NOTE = ut('ja').detail.groundingNote;

/** 実行の結果の 1 文。失敗は理由つき。 */
export function resultText(r: ReviewView, lang: Lang = getAppLang()): string {
  const t = ut(lang).detail;
  if (r.error) return t.resultFailed(r.error);
  if (r.parse_error) return t.resultParseError(r.parse_error);
  return t.resultOk;
}

export function detailPanel(r: ReviewView, fmt: DetailFmt, lang: Lang = getAppLang()): HTMLElement {
  const t = ut(lang).detail;
  const nf = new Intl.NumberFormat(msg(lang).bcp47);
  const table = (...rows: ([string, string] | null)[]) =>
    h(
      'table',
      { class: 'kvt' },
      ...rows.filter((x): x is [string, string] => x !== null).map(([k, v]) => h('tr', {}, h('th', {}, k), h('td', {}, v))),
    );
  const u = r.usage;
  const total = u.input_tokens === null && u.output_tokens === null ? null : (u.input_tokens ?? 0) + (u.output_tokens ?? 0);

  const count = (s: string) => r.triage.filter((x) => x === s).length;
  const verified = r.findings.filter((f) => f.verified).length;
  const specVerified = r.findings.filter((f) => f.spec_verified === true).length;
  // 参照した基準文書の全部（古い記録・1 件だけのものは `spec` が全部）。複数あるときは、1 件ずつ行にする
  const specDocs = r.specs && r.specs.length > 0 ? r.specs : r.spec ? [r.spec] : [];
  const specRows = specDocs.map((d, i): [string, string] => [
    specDocs.length > 1 ? `${t.specDoc} ${i + 1}` : t.specDoc,
    t.specChars(d.display_name, nf.format(d.char_count)),
  ]);
  const hasSpec = specDocs.length > 0 || r.findings.some((f) => f.spec_quote);
  const done = (r.finding_notes ?? []).filter((n) => n.done_at !== null).length;
  const reports = r.agent_reports ?? [];
  const did = reports.filter((a) => a?.verdict === 'did').length;
  const skipped = reports.filter((a) => a?.verdict === 'skipped').length;
  // 保存元: `mrcr handoff report`（CLI）で保存したものと、会話の記録（ログ）から取り込んだもの、MCP で保存したもの
  const bySource = (k: 'cli' | 'mcp' | 'log') => reports.filter((a) => a && agentSourceKind(a.source) === k).length;
  const sources = [
    bySource('cli') > 0 ? t.srcCli(bySource('cli')) : '',
    bySource('mcp') > 0 ? t.srcMcp(bySource('mcp')) : '',
    bySource('log') > 0 ? t.srcLog(bySource('log')) : '',
  ]
    .filter(Boolean)
    .join(t.sourcesJoin);

  return h(
    'div',
    { class: 'detailtab' },
    h('h3', {}, t.hRun),
    table(
      [t.date, fmt.when(r.created_at)],
      [t.duration, fmt.dur(r.duration_ms)],
      [t.llm, providerLabel(r.provider, lang)],
      [t.model, r.model || t.notReported],
      [t.result, resultText(r, lang)],
    ),
    h('h3', {}, t.hScope),
    table(
      [t.range, fmt.base || t.uncommitted],
      [t.head, r.snapshot.head],
      ...specRows,
      [t.diffId, r.snapshot.diff_hash],
      [t.diffSize, `${nf.format(r.snapshot.diff_bytes)} B`],
      [t.filesSent, t.items(r.snapshot.files.length)],
      [t.excluded, t.items(r.snapshot.excluded.length)],
      [t.prompt, t.promptChars(nf.format(r.prompt.length))],
    ),
    h('h3', {}, t.hTokens),
    table(
      [t.tokIn, fmt.num(u.input_tokens)],
      [t.tokOut, fmt.num(u.output_tokens)],
      [t.cacheRead, fmt.num(u.cache_read_tokens)],
      [t.cacheWrite, fmt.num(u.cache_write_tokens)],
      [t.tokTotal, fmt.num(total)],
      [t.cost, r.cost_usd === null ? t.notReported : `$${r.cost_usd.toFixed(4)}`],
    ),
    r.provider.id === 'tool' ? h('p', { class: 'note' }, t.toolTokens) : null,
    h('p', { class: 'note' }, t.tokenNote),
    h('h3', {}, t.hFindings),
    table(
      [t.fCount, t.items(r.findings.length)],
      [t.fGrounded, t.fGroundedValue(verified, r.findings.length - verified)],
      hasSpec ? [t.specCheck, t.specCheckValue(specVerified, r.findings.filter((f) => f.spec_verified === false).length)] : null,
      [t.triage, t.triageValue(count('fix'), count('wait'), count('dismiss'), count('new'))],
      [t.doneByYou, t.items(done)],
      [t.agentReports, reports.some(Boolean) ? t.agentReportsValue(did, skipped, sources) : t.noReports],
    ),
    h('p', { class: 'note' }, `${t.groundingNote}${r.provider.id === 'tool' ? t.toolGrounding : ''}`),
    h('h3', {}, t.hRecord),
    table([t.id, reviewTag(r.id)], [t.runNotes, r.notes.length ? r.notes.join(' ／ ') : t.none]),
  );
}
