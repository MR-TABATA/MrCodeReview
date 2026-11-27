import { h } from './dom';
import { msg, type Lang } from './i18n';
import { extractDiff } from './diffview';
import type { ReviewView } from './types';

/**
 * ログの 1 行を開いたときの、実行の詳細（指摘は、ここには出さない。結果は、結果のページで見る）。
 * 「実行」: 観点・プロバイダ・モデル・所要時間・トークン・費用。「入力」: 比較元・差分・設計書・関連コード・プロンプト。
 * 「結果の内容」: 指摘の本文・引用・対応内容・対象ファイル（読むだけ。仕分けやメモの編集は、結果のページ）。
 * 「レビューに送った差分」: 記録（プロンプト）の中の差分。
 */
export function runLogDetail(r: ReviewView, lang: Lang, onOpenResult: () => void): HTMLElement {
  const t = msg(lang).mainUi.runLogDetail;
  const hm = msg(lang).hookUi;
  const mu = msg(lang).mainUi;
  const stateLabel = new Map(mu.triageLabels);
  const row = (label: string, value: string | Node): HTMLElement =>
    h('div', { class: 'rld-row' }, h('span', { class: 'muted rld-label' }, label), h('span', { class: 'rld-value' }, value));

  const base = r.snapshot.base;
  const baseText = !base || base.spec === 'HEAD' ? t.baseWorking : t.baseCommit(base.spec, base.subject);
  const specs = r.specs?.length ? r.specs : r.spec ? [r.spec] : [];
  const u = r.usage;
  const hasTokens = u.input_tokens !== null || u.output_tokens !== null;

  return h(
    'div',
    { class: 'runlog-card-detail rld', onclick: (e: Event) => e.stopPropagation() },
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
    h('div', { class: 'rld-group rld-wide' }, h('h4', {}, t.groupResult), ...resultItems(r, t, stateLabel)),
    diffBlock(r, t),
    h('div', { class: 'rld-foot' }, h('button', { class: 'small primary', onclick: onOpenResult }, t.openResult)),
  );
}

type Dict = ReturnType<typeof msg>['mainUi']['runLogDetail'];

/** 指摘 1 件ずつ（読むだけ）。 */
function resultItems(r: ReviewView, t: Dict, stateLabel: Map<string, string>): HTMLElement[] {
  if (!r.findings.length) return [h('p', { class: 'muted rld-nofindings' }, r.error ? r.error : t.noFindings)];
  return r.findings.map((f, i) => {
    const st = r.triage[i] ?? 'new';
    const memo = r.finding_notes?.[i]?.memo?.trim();
    const where = f.line === null || f.line === undefined ? f.file : `${f.file}:${f.line}`;
    return h(
      'div',
      { class: `rld-finding sev-${f.severity}` },
      h(
        'div',
        { class: 'rld-finding-head' },
        h('span', { class: 'muted' }, `#${i + 1}`),
        h('span', { class: `badge sev-${f.severity}` }, f.severity),
        h('strong', {}, f.title),
        h('span', { class: 'badge rld-state' }, stateLabel.get(st) ?? st),
      ),
      h('div', { class: 'rld-row' }, h('span', { class: 'muted rld-label' }, t.target), h('code', { class: 'rld-value' }, where)),
      f.detail ? h('p', { class: 'rld-detail' }, f.detail) : null,
      f.evidence
        ? h('div', { class: 'rld-quote' }, h('span', { class: 'muted rld-label' }, t.quote), h('pre', { class: 'rld-pre' }, f.evidence))
        : null,
      h('div', { class: 'rld-row' }, h('span', { class: 'muted rld-label' }, t.handling), h('span', { class: 'rld-value' }, memo || t.handlingNone)),
    );
  });
}

/** レビューに送った差分。記録のプロンプトから取り出す（開いたときだけ描く）。 */
function diffBlock(r: ReviewView, t: Dict): HTMLElement {
  const diff = extractDiff(r.prompt);
  return h(
    'details',
    { class: 'rld-wide rld-diff' },
    h('summary', {}, t.diffSummary),
    diff === null ? h('p', { class: 'muted' }, t.diffMissing) : h('pre', { class: 'rld-pre rld-diff-pre' }, diff),
  );
}
