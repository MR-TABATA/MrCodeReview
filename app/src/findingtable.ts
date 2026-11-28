import { h } from './dom';
import { countsLine } from './historyview';
import { msg, type Lang } from './i18n';
import type { FindingStats } from './findingstats';

/** 表の 1 行ぶん。中身の部品（印・チップ・詳細）は、呼ぶ側が作る（仕分け・申告・メモなどの動作は、今のカードのまま）。 */
export interface FindingRow {
  index: number;
  severity: string;
  /** 仕分けの状態（new・fix・wait・dismiss）。行の色に使う */
  status: string;
  verified: boolean;
  /** 対応済み（薄くする） */
  done: boolean;
  open: boolean;
  /** 「#3」の印 */
  no: Node;
  /** 重大度のバッジ */
  sev: Node;
  /** 判断の列: 仕分けの印と、申告・対応済み・メモの小さな印 */
  statusPill: Node;
  statusChips: Node[];
  title: string;
  /** 内容の 1 行目 */
  peek: string;
  /** 見出しの下の、小さな印（根拠不一致・規則・引き継ぎ など） */
  subChips: Node[];
  /** ファイル:行 */
  loc: string;
  onToggle: () => void;
  /** 開いているときの詳細（仕分けのボタン・全文・引用・申告・メモ） */
  detail: Node | null;
}

/** 指摘の表。行を押すと、その行の下に、詳細が開く（同時に開くのは 1 件だけ。開閉は、呼ぶ側が決める）。 */
export function findingsTable(rows: FindingRow[], lang: Lang): HTMLElement {
  const m = msg(lang).mainUi.findingTable;
  return h(
    'div',
    { class: 'ftable', role: 'table', 'aria-label': m.tableLabel },
    h(
      'div',
      { class: 'frow frow-head', role: 'row' },
      h('span', { class: 'fno' }),
      h('span', {}, m.colStatus),
      h('span', {}, m.colSeverity),
      h('span', {}, m.colFinding),
      h('span', {}, m.colWhere),
    ),
    ...rows.map((r) =>
      h(
        'article',
        {
          class: `finding fc fitem ${r.verified ? '' : 'unverified'} s-${r.status} fc-sev-${r.severity} ${r.open ? 'open' : ''} ${r.done ? 'fdone' : ''}`.replace(/\s+/g, ' ').trim(),
          onclick: (e: Event) => e.stopPropagation(),
        },
        h(
          'div',
          {
            class: 'frow',
            role: 'button',
            tabindex: '0',
            'aria-expanded': String(r.open),
            onclick: r.onToggle,
            onkeydown: (ev: Event) => {
              const e = ev as KeyboardEvent;
              if (e.target !== e.currentTarget) return; // 中のボタンのキー操作で、開閉しない
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                r.onToggle();
              }
            },
          },
          r.no,
          h('div', { class: 'fstat' }, r.statusPill, ...r.statusChips),
          h('span', { class: 'fsev' }, r.sev),
          h('div', { class: 'fmain' }, h('strong', { class: 'ftitle' }, r.title), h('div', { class: 'fpeek' }, r.open ? null : h('span', { class: 'fc-peek' }, r.peek), ...r.subChips)),
          h('code', { class: 'floc', title: r.loc }, r.loc),
        ),
        r.open && r.detail ? h('div', { class: 'fdetail' }, r.detail) : null,
      ),
    ),
  );
}

/**
 * 指摘の集計の帯。
 * 簡易版（履歴・ログの行の下）: 1 行（指摘の件数・重大度 ／ 判断の内訳）。行の高さを取りすぎないように。
 * フル版（新規レビューの下）: 指摘・high・medium・low の 4 枚と、判断の内訳。
 */
export function findingsSummary(stats: FindingStats, compact: boolean, lang: Lang): HTMLElement | null {
  if (stats.total === 0) return null;
  const m = msg(lang).mainUi.findingTable;
  const sev = stats.severity;
  const tally = countsLine(stats.counts, lang);
  if (compact) {
    return h(
      'div',
      { class: 'fsum fsum-compact' },
      h('span', { class: 'fsum-line' }, m.summaryLine(stats.total, sev.high, sev.medium, sev.low)),
      tally,
    );
  }
  const card = (cls: string, label: string, n: number): HTMLElement => h('div', { class: `fsum-card ${cls}` }, h('div', { class: 'fsum-t' }, label), h('div', { class: 'fsum-n' }, String(n)));
  return h(
    'div',
    { class: 'fsum' },
    h('div', { class: 'fsum-cards' }, card('', m.cardTotal, stats.total), card('sev-high', 'high', sev.high), card('sev-medium', 'medium', sev.medium), card('sev-low', 'low', sev.low)),
    tally,
  );
}

/** 結果のページの上の、実行の情報 5 枚: 状態・モデル・観点・入力トークン・所要時間。 */
export function runInfoCards(info: { failed: boolean; model: string; aspect: string; inputTokens: number | null; durationText: string }, lang: Lang): HTMLElement {
  const m = msg(lang).mainUi.findingTable;
  const card = (label: string, value: Node | string, cls = ''): HTMLElement => h('div', { class: `fsum-card ${cls}`.trim() }, h('div', { class: 'fsum-t' }, label), h('div', { class: 'fsum-v' }, value));
  return h(
    'div',
    { class: 'fsum-cards fsum-info' },
    card(m.infoStatus, h('span', { class: `badge ${info.failed ? 'bad' : 'ok'}` }, info.failed ? m.infoStatusFailed : m.infoStatusDone)),
    card(m.infoModel, info.model || '—'),
    card(m.infoAspect, info.aspect),
    card(m.infoTokens, info.inputTokens === null ? '—' : info.inputTokens.toLocaleString(msg(lang).bcp47)),
    card(m.infoDuration, info.durationText),
  );
}
