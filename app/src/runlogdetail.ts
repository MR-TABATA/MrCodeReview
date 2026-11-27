import { h } from './dom';
import { msg, type Lang } from './i18n';
import { extractDiff } from './diffview';
import type { ReviewView } from './types';

/**
 * 結果のページの下に置く「実行の記録」（指摘は、ここには出さない。結果は、結果のページ本体にある）。
 * 「実行」: 観点・プロバイダ・モデル・所要時間・トークン・費用。「入力」: 比較元・差分・設計書・関連コード・プロンプト。
 * 「レビューに送った差分」: 記録（プロンプト）の中の差分。
 */
export function runLogDetail(
  r: ReviewView,
  lang: Lang,
  /** 差分の欄の開閉。開いているとき（と、開いたとき）にだけ、差分を取り出す */
  diff: { open?: boolean; onToggle?: (open: boolean) => void } = {},
): HTMLElement {
  const t = msg(lang).mainUi.runLogDetail;
  const hm = msg(lang).hookUi;
  const row = (label: string, value: string | Node): HTMLElement =>
    h('div', { class: 'rld-row' }, h('span', { class: 'muted rld-label' }, label), h('span', { class: 'rld-value' }, value));

  const base = r.snapshot.base;
  const baseText = !base || base.spec === 'HEAD' ? t.baseWorking : t.baseCommit(base.spec, base.subject);
  const specs = r.specs?.length ? r.specs : r.spec ? [r.spec] : [];
  const u = r.usage;
  const hasTokens = u.input_tokens !== null || u.output_tokens !== null;

  return h(
    'div',
    { class: 'rld' },
    h(
      'div',
      { class: 'rld-group' },
      h('h4', {}, t.groupRun),
      row(t.recipe, r.recipe.title),
      row(t.provider, `${r.provider.label} · ${r.model}`),
      row(t.duration, hm.secs(r.duration_ms)),
      row(t.tokens, hasTokens ? t.tokensValue(u.input_tokens, u.output_tokens) : t.none),
      r.cost_usd !== null ? row(t.cost, `$${r.cost_usd.toFixed(4)}`) : null,
      r.error ? row(t.failure, r.error) : null,
    ),
    h(
      'div',
      { class: 'rld-group' },
      h('h4', {}, t.groupInput),
      row(t.base, baseText),
      row(t.diff, t.diffValue(r.snapshot.files.length, r.snapshot.diff_bytes)),
      row(t.specs, specs.length ? specs.map((s) => s.display_name).join('、') : t.none),
      r.related ? row(t.related, t.relatedValue(r.related.snippets.length)) : null,
      row(t.prompt, t.promptValue(r.prompt.length)),
    ),
    diffBlock(r, t, diff.open ?? false, diff.onToggle),
  );
}

type Dict = ReturnType<typeof msg>['mainUi']['runLogDetail'];

/**
 * レビューに送った差分。記録のプロンプトから取り出す。
 * 閉じているあいだは、取り出さない（差分が大きいと、行を開閉するたびに、開いている全部の行で、作り直すことになるため）。
 */
function diffBlock(r: ReviewView, t: Dict, open: boolean, onToggle?: (open: boolean) => void): HTMLElement {
  const det = h('details', { class: 'rld-wide rld-diff' }, h('summary', {}, t.diffSummary)) as HTMLDetailsElement;
  const fill = (): void => {
    if (det.querySelector('.rld-diff-body')) return;
    const diff = extractDiff(r.prompt);
    det.append(h('div', { class: 'rld-diff-body' }, diff === null ? h('p', { class: 'muted' }, t.diffMissing) : h('pre', { class: 'rld-pre rld-diff-pre' }, diff)));
  };
  if (open) {
    det.open = true;
    fill();
  }
  det.addEventListener('toggle', () => {
    onToggle?.(det.open);
    if (det.open) fill();
  });
  return det;
}
