// 結果の「詳細」タブ。1 回のレビューについて、記録している値を、ぜんぶ並べる（旧「トークン」タブも、ここに含める）。
//
// - 値は、記録した実測値だけ。提供元が報告しなかった項目は、推定で埋めず、「（報告なし）」と言う。
// - 履歴の一覧は、目次に絞った。細かい数字は、ここに集める。あとで、観点別・LLM 別などの統計に育てるときの、元になる項目でもある。

import { agentSourceKind } from './agentview';
import { reviewTag } from './findingtag';
import { h } from './dom';
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

const nf = new Intl.NumberFormat('ja-JP');

/** 「根拠あり／根拠不一致」の意味。この言葉が出る場所（詳細タブの指摘の表・カードの引用・「⚠ 根拠不一致」）に、添える */
export const GROUNDING_NOTE = '根拠の確認は、引用が差分に文字列として存在するかだけを見ています。指摘が正しいかどうかの判断ではありません。';

/** 実行の結果の 1 文。失敗は理由つき。 */
export function resultText(r: ReviewView): string {
  if (r.error) return `失敗: ${r.error}`;
  if (r.parse_error) return `実行は成功。指摘を読めなかった: ${r.parse_error}`;
  return '成功';
}

export function detailPanel(r: ReviewView, fmt: DetailFmt): HTMLElement {
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
  const hasSpec = !!r.spec || r.findings.some((f) => f.spec_quote);
  const done = (r.finding_notes ?? []).filter((n) => n.done_at !== null).length;
  const reports = r.agent_reports ?? [];
  const did = reports.filter((a) => a?.verdict === 'did').length;
  const skipped = reports.filter((a) => a?.verdict === 'skipped').length;
  // 保存元: `mrcr handoff report`（CLI）で保存したものと、会話の記録（ログ）から取り込んだもの、MCP で保存したもの
  const bySource = (k: 'cli' | 'mcp' | 'log') => reports.filter((a) => a && agentSourceKind(a.source) === k).length;
  const sources = [bySource('cli') > 0 ? `mrcr handoff report で保存 ${bySource('cli')} 件` : '', bySource('mcp') > 0 ? `MCP で保存 ${bySource('mcp')} 件` : '', bySource('log') > 0 ? `会話の記録から取り込み ${bySource('log')} 件` : '']
    .filter(Boolean)
    .join('・');

  return h(
    'div',
    { class: 'detailtab' },
    h('h3', {}, '実行'),
    table(
      ['日時', fmt.when(r.created_at)],
      ['所要時間', fmt.dur(r.duration_ms)],
      ['LLM', r.provider.label],
      ['モデル', r.model || '（報告なし）'],
      ['結果', resultText(r)],
    ),
    h('h3', {}, '見る範囲と、送った内容'),
    table(
      ['見る範囲', fmt.base || '未コミットの変更（HEAD から）'],
      ['HEAD', r.snapshot.head],
      r.spec ? ['基準の文書', `${r.spec.display_name}（${nf.format(r.spec.char_count)} 文字）`] : null,
      ['差分の識別子', r.snapshot.diff_hash],
      ['差分の大きさ', `${nf.format(r.snapshot.diff_bytes)} B`],
      ['送ったファイル', `${r.snapshot.files.length} 件`],
      ['送らなかったもの', `${r.snapshot.excluded.length} 件`],
      ['プロンプト', `${nf.format(r.prompt.length)} 文字`],
    ),
    h('h3', {}, 'トークン'),
    table(
      ['入力', fmt.num(u.input_tokens)],
      ['出力', fmt.num(u.output_tokens)],
      ['キャッシュ読み取り', fmt.num(u.cache_read_tokens)],
      ['キャッシュ書き込み', fmt.num(u.cache_write_tokens)],
      ['入力＋出力', fmt.num(total)],
      ['提供元の報告額（参考）', r.cost_usd === null ? '（報告なし）' : `$${r.cost_usd.toFixed(4)}`],
    ),
    r.provider.id === 'tool' ? h('p', { class: 'note' }, 'LLM を呼んでいないので、トークンは 0 です（測っての 0 で、報告なしではありません）。') : null,
    h('p', { class: 'note' }, 'トークンは提供元が返した実測値だけを記録します。報告が無い項目は、推定で埋めずに空欄のままにします。金額は、提供元の報告額を参考に出すだけです（サブスクでは請求されません）。'),
    h('h3', {}, '指摘'),
    table(
      ['件数', `${r.findings.length} 件`],
      ['根拠あり／根拠不一致', `${verified} 件／${r.findings.length - verified} 件`],
      hasSpec ? ['設計書の根拠照合', `${specVerified} 件照合済／${r.findings.filter((f) => f.spec_verified === false).length} 件不一致`] : null,
      ['仕分け', `対応 ${count('fix')} ・待ち ${count('wait')} ・無視 ${count('dismiss')} ・未仕分け ${count('new')}`],
      ['対応済み（あなたの操作）', `${done} 件`],
      ['エージェントの申告', reports.some(Boolean) ? `対応した ${did} 件 ・対応しなかった ${skipped} 件（${sources}。どれも、エージェントの申告・判断で、MCR が確かめたものではありません）` : '（申告はありません）'],
    ),
    h('p', { class: 'note' }, `${GROUNDING_NOTE}${r.provider.id === 'tool' ? '（linter の指摘は、引用が差分の行そのものなので、必ず根拠ありになります）' : ''}`),
    h('h3', {}, '記録'),
    table(['識別子', reviewTag(r.id)], ['実行メモ', r.notes.length ? r.notes.join(' ／ ') : 'なし']),
  );
}
