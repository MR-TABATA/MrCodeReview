// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest';
import { findingsSummary, findingsTable, type FindingRow } from './findingtable';
import { findingStats } from './findingstats';

const row = (over: Partial<FindingRow> = {}): FindingRow => ({
  index: 0,
  severity: 'high',
  status: 'new',
  verified: true,
  done: false,
  open: false,
  no: document.createTextNode('#1'),
  sev: document.createTextNode('▲ high'),
  statusPill: document.createTextNode('未仕分け'),
  statusChips: [],
  title: '見出し',
  peek: '内容の 1 行目',
  subChips: [],
  loc: 'src/a.rs:3',
  onToggle: () => {},
  detail: document.createTextNode('詳細'),
  ...over,
});

describe('指摘の表', () => {
  it('見出しの行（判断・重大度・指摘・場所）と、指摘ごとの行。閉じているときは、詳細を出さず、内容の 1 行目を出す', () => {
    const el = findingsTable([row(), row({ index: 1, title: '2 つ目' })], 'ja');
    expect([...el.querySelectorAll('.frow-head span')].map((s) => s.textContent).filter(Boolean)).toEqual(['判断', '重大度', '指摘', '場所']);
    expect(el.querySelectorAll('.fitem').length).toBe(2);
    expect(el.querySelector('.fitem .ftitle')?.textContent).toBe('見出し');
    expect(el.querySelector('.fitem .fc-peek')?.textContent).toBe('内容の 1 行目');
    expect(el.querySelector('.floc')?.textContent).toBe('src/a.rs:3');
    expect(el.querySelector('.fdetail')).toBeNull();
  });

  it('開いている行にだけ、行の下に詳細が出る（内容の 1 行目は、出さない）。押すと、開閉の関数が呼ばれる。Enter でも', () => {
    const onToggle = vi.fn();
    const el = findingsTable([row({ open: true, onToggle }), row({ index: 1 })], 'ja');
    const items = el.querySelectorAll('.fitem');
    expect(items[0].querySelector('.fdetail')?.textContent).toBe('詳細');
    expect(items[0].querySelector('.fc-peek')).toBeNull();
    expect(items[1].querySelector('.fdetail')).toBeNull();
    (items[0].querySelector('.frow') as HTMLElement).click();
    expect(onToggle).toHaveBeenCalledTimes(1);
    (items[0].querySelector('.frow') as HTMLElement).dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
    expect(onToggle).toHaveBeenCalledTimes(2);
    expect((items[0].querySelector('.frow') as HTMLElement).getAttribute('aria-expanded')).toBe('true');
  });

  it('詳細の中を押しても、開閉しない。対応済みの行は薄くする印（fdone）、重大度・仕分けは、クラスに出る', () => {
    const onToggle = vi.fn();
    const el = findingsTable([row({ open: true, onToggle, done: true, status: 'fix', severity: 'low' })], 'ja');
    (el.querySelector('.fdetail') as HTMLElement).click();
    expect(onToggle).not.toHaveBeenCalled();
    const item = el.querySelector('.fitem')!;
    expect(item.classList.contains('fdone')).toBe(true);
    expect(item.classList.contains('s-fix')).toBe(true);
    expect(item.classList.contains('fc-sev-low')).toBe(true);
  });
});

describe('指摘の集計の帯', () => {
  const stats = findingStats({
    findings: [{ severity: 'high' }, { severity: 'medium' }, { severity: 'medium' }],
    triage: ['new', 'fix', 'dismiss'],
    agent_reports: [null, { verdict: 'skipped', text: 'x', reported_at: 1, source: 's' }, null],
  });

  it('簡易版は 1 行（件数・重大度 ＋ 判断の内訳）。判断待ちは、あるときだけ出る', () => {
    const el = findingsSummary(stats, true, 'ja')!;
    expect(el.querySelector('.fsum-line')?.textContent).toBe('指摘 3 件 ・ high 1 ・ medium 2 ・ low 0');
    expect(el.querySelector('.fsum-cards')).toBeNull();
    expect([...el.querySelectorAll('.hc')].map((c) => c.textContent?.split(' ')[0])).toEqual(['未', '判断待ち', '済み', '待ち', '無視']);
  });

  it('フル版は、指摘・high・medium・low の 4 枚と、判断の内訳', () => {
    const el = findingsSummary(stats, false, 'ja')!;
    expect([...el.querySelectorAll('.fsum-card')].map((c) => c.textContent)).toEqual(['指摘3', 'high1', 'medium2', 'low0']);
    expect(el.querySelector('.hcounts')).not.toBeNull();
  });

  it('指摘が 0 件なら、出さない', () => {
    expect(findingsSummary(findingStats({ findings: [], triage: [] }), true, 'ja')).toBeNull();
  });
});
