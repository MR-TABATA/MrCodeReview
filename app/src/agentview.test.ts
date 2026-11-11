// @vitest-environment jsdom
import { describe, it, expect, vi } from 'vitest';
import {
  agentBox,
  agentDone,
  agentFetchBar,
  agentNotice,
  agentSourceKind,
  agentSourceLabel,
  agentStatus,
  agentStatusBar,
  agentSummary,
  foldedRow,
  newAgentUi,
  refreshAgentReportsOnFocus,
  type AgentRefreshDeps,
  type AgentUi,
} from './agentview';
import type { AgentFetch, AgentReport, Note } from './types';

const FETCH: AgentFetch = { needs_consent: false, found: 2, updated: 2, files_scanned: 5, files_with_tag: 1, unreadable_lines: 0, ignored: 0, skipped_files: 0, agent_reports: [] };
const REPORT: AgentReport = { verdict: 'did', text: '境界を直した（lib.rs）', reported_at: 1_793_074_422_568, source: 'sess1' };
const acts = () => ({ fetch: vi.fn(), cancel: vi.fn() });
const ui = (over: Partial<AgentUi> = {}): AgentUi => ({ ...newAgentUi(), ...over });
const btn = (el: HTMLElement, label: string) => [...el.querySelectorAll('button')].find((b) => b.textContent === label) as HTMLButtonElement | undefined;

describe('agentFetchBar（「対応内容を取り込む」）', () => {
  it('押すと、許可なしで、取り込みを始める（許可が要るかは、バックエンドが答える）', () => {
    const a = acts();
    btn(agentFetchBar(ui(), a), '対応内容を取り込む')!.click();
    expect(a.fetch).toHaveBeenCalledExactlyOnceWith(false);
  });
  it('取り込み中は、押せない', () => {
    const b = btn(agentFetchBar(ui({ busy: true }), acts()), '取り込んでいます…')!;
    expect(b.disabled).toBe(true);
  });
});

describe('agentNotice（結果・許可の確認・失敗）', () => {
  it('何も無ければ、何も出さない。許可を聞くときは返す結果を出さない', () => {
    expect(agentNotice(ui(), acts())).toBeNull();
    expect(agentNotice(ui({ result: { ...FETCH, needs_consent: true } }), acts())).toBeNull();
  });
  it('初めては、何を読み、何を保存するかを言ってから、聞く。「読む」で許可つきで取り込み、「やめる」で取りやめる', () => {
    const a = acts();
    const el = agentNotice(ui({ asking: true }), a)!;
    expect(el.textContent).toContain('会話の記録（Claude Code・agy・GitHub Copilot CLI のログ）を読んでよいですか？');
    expect(el.textContent).toContain('このレビューの識別子を含む、エージェントの返事の行だけ');
    expect(el.textContent).toContain('会話の本文は、保存しません');
    btn(el, '読む')!.click();
    expect(a.fetch).toHaveBeenCalledExactlyOnceWith(true);
    btn(el, 'やめる')!.click();
    expect(a.cancel).toHaveBeenCalledOnce();
  });
  it('失敗したときは、理由を出す（黙って空にしない）', () => {
    const el = agentNotice(ui({ error: 'Claude Code のログの置き場所を開けません: /x' }), acts())!;
    expect(el.className).toBe('err');
    expect(el.textContent).toContain('ログの置き場所を開けません');
  });
  it('結果の文は、取り込めたかどうかで、見た目の印を変える', () => {
    expect(agentNotice(ui({ result: FETCH }), acts())!.className).toBe('muted agentsum');
    expect(agentNotice(ui({ result: { ...FETCH, found: 0, updated: 0 } }), acts())!.className).toBe('muted agentsum none');
  });
});

describe('agentSummary', () => {
  it('上限のために調べなかったログがあれば、件数で言う。見つからなかったときも、「調べていない」と取り違えない', () => {
    const t = agentSummary({ ...FETCH, found: 0, updated: 0, skipped_files: 12 });
    expect(t).toContain('報告は見つかりませんでした');
    expect(t).toContain('上限を超えたため、古いログ 12 件は、調べていません');
    expect(agentSummary({ ...FETCH, skipped_files: 3 })).toContain('古いログ 3 件は、調べていません');
    expect(agentSummary(FETCH), '0 件なら、言わない').not.toContain('調べていません');
  });

  it('取り込めたら、件数と、更新の有無を言う', () => {
    expect(agentSummary(FETCH)).toBe('2 件の報告を取り込みました（2 件を、新しく保存・更新）。');
    expect(agentSummary({ ...FETCH, updated: 0 })).toBe('2 件の報告を取り込みました（変更はありません）。');
  });
  it('何も見つからなかったときは、推定せず、調べた数と、考えられる理由を言う', () => {
    const t = agentSummary({ ...FETCH, found: 0, updated: 0, files_scanned: 7, files_with_tag: 0 });
    expect(t).toContain('報告は見つかりませんでした（調べたログ 7 件、識別子を含むログ 0 件）');
    expect(t).toContain('「最後の報告」を書いていません');
    expect(t, '未対応のエージェントのときも、理由に気づけるように、読めるログの種類を言う').toContain('読めるのは、Claude Code・agy・Copilot CLI のログだけ');
  });
  it('番号違いの報告と、読めなかった行は、黙らずに、件数で言う', () => {
    const t = agentSummary({ ...FETCH, ignored: 2, unreadable_lines: 3 });
    expect(t).toContain('このレビューに無い番号の報告が 2 件ありました（取り込んでいません）。');
    expect(t).toContain('読めなかった行が 3 件あります');
    expect(agentSummary(FETCH)).not.toContain('読めなかった');
  });
});

describe('agentBox（カードの中の「エージェントの申告」）', () => {
  const when = (ms: number) => `T${ms}`;
  it('報告が無ければ、何も出さない（「記録なし」と、文字にもしない）', () => {
    expect(agentBox(null, when)).toBeNull();
    expect(agentBox(undefined, when)).toBeNull();
  });
  it('「対応した」の報告: 申告であることと、実際に直ったかの確認ではないことを、必ず添える', () => {
    const el = agentBox(REPORT, when)!;
    expect(el.className).toBe('agentrep did');
    expect(el.querySelector('.badge')?.textContent).toBe('対応した');
    expect(el.querySelector('strong')?.textContent).toBe('エージェントの申告');
    expect(el.querySelector('.agenttext')?.textContent).toBe('境界を直した（lib.rs）');
    expect(el.textContent).toContain(`T${REPORT.reported_at} ・ ログから取り込み`);
    // 何の、誰の確認ではないのかを言う（主語と対象のない「確認ではありません」は、意味が取れない）
    expect(el.textContent).toContain('これは、エージェントの申告です。MCR は、コードが実際に直ったかを、確かめていません。');
    expect(el.textContent).not.toContain('実際に直ったかの確認ではありません');
  });
  it('「対応しなかった」の報告は、別の見た目の印で、理由を出す', () => {
    const el = agentBox({ ...REPORT, verdict: 'skipped', text: '既に直っていた' }, when)!;
    expect(el.className).toBe('agentrep skipped');
    expect(el.querySelector('.badge')?.textContent).toBe('対応しなかった');
    expect(el.querySelector('.agenttext')?.textContent).toBe('既に直っていた');
  });
  it('「対応しなかった」には、「直ったか」とは言わない（直していないので）。判断が正しいかを確かめていない、と言い、あなたが決めると言う', () => {
    const t = agentBox({ ...REPORT, verdict: 'skipped', text: '誤検知です' }, when)!.textContent!;
    expect(t).toContain('これは、エージェントの判断です。MCR は、その判断が正しいかを、確かめていません。読んで、あなたが決めてください。');
    expect(t).not.toContain('直ったか');
    expect(t, '「もう一度レビュー」の案内は、「直した」ときだけ').not.toContain('確かめるには');
    // 「直した」には、直ったかを確かめていない、と言う
    expect(agentBox(REPORT, when)!.textContent).toContain('コードが実際に直ったかを、確かめていません');
  });
  it('報告の文章は、文字として出す（HTML として解釈しない）', () => {
    const el = agentBox({ ...REPORT, text: '<img src=x onerror=alert(1)>' }, when)!;
    expect(el.querySelector('img')).toBeNull();
    expect(el.querySelector('.agenttext')?.textContent).toBe('<img src=x onerror=alert(1)>');
  });
});

describe('agentDone（畳むかどうか）', () => {
  it('「対応した」と申告された「対応」の指摘だけ、終わったものとして、畳む', () => {
    expect(agentDone(REPORT, 'fix')).toBe(true);
    expect(agentDone({ ...REPORT, verdict: 'skipped' }, 'fix'), '「対応しなかった」は、畳まない（判断が、まだ残っている）').toBe(false);
    expect(agentDone(REPORT, 'wait'), '使う人が、仕分けを変えたものは、畳まない').toBe(false);
    expect(agentDone(REPORT, 'dismiss')).toBe(false);
    expect(agentDone(REPORT, 'new')).toBe(false);
    expect(agentDone(null, 'fix')).toBe(false);
    expect(agentDone(undefined, 'fix')).toBe(false);
  });
});

describe('foldedRow（畳んだ 1 行）', () => {
  it('対応済みにした人で、文言が変わる: あなたが付けたものは「（あなたの申告）」、エージェントのものは「（エージェントの申告）」つき', () => {
    expect(foldedRow({ severity: 'low', title: 'x' }, vi.fn()).querySelector('.foldmark')!.textContent).toBe('✓ 対応済み（エージェントの申告）');
    expect(foldedRow({ severity: 'low', title: 'x' }, vi.fn(), undefined, 'you').querySelector('.foldmark')!.textContent).toBe('✓ 対応済み（あなたの申告）');
  });

  it('番号を渡すと、行の先頭に #番号 が出て、識別子の全文が、ホバーに出る（エージェントの報告と照合できる）', () => {
    const el = foldedRow({ severity: 'low', title: 'x' }, vi.fn(), { index: 4, tag: 'mrcr:r#5' });
    expect(el.querySelector('.fc-no')!.textContent).toBe('#5');
    expect((el.querySelector('.fc-no') as HTMLElement).title).toBe('識別子: mrcr:r#5');
    expect((el.querySelector('button') as HTMLElement).title).toContain('mrcr:r#5');
    expect(foldedRow({ severity: 'low', title: 'x' }, vi.fn()).querySelector('.fc-no')).toBeNull();
  });

  it('重大度・見出し・「対応済み（エージェントの申告）」だけを出す。操作のボタンは、出さない', () => {
    const el = foldedRow({ severity: 'medium', title: '同期コマンドで、全レビューの JSON を読んでいる' }, vi.fn());
    expect(el.className).toBe('finding folded s-fix');
    expect(el.querySelector('.badge')?.textContent).toBe('medium');
    expect(el.querySelector('strong')?.textContent).toBe('同期コマンドで、全レビューの JSON を読んでいる');
    expect(el.querySelector('.foldmark')?.textContent).toBe('✓ 対応済み（エージェントの申告）');
    expect(el.querySelectorAll('button')).toHaveLength(1);
    for (const w of ['CLI に渡す', '対応メモ', '対応済みにする', '待ち', '無視']) expect(el.textContent).not.toContain(w);
  });
  it('押すと、開く。見出しが空なら、「(見出しなし)」', () => {
    const open = vi.fn();
    const el = foldedRow({ severity: 'low', title: '' }, open);
    expect(el.querySelector('strong')?.textContent).toBe('(見出しなし)');
    (el.querySelector('button') as HTMLElement).click();
    expect(open).toHaveBeenCalledOnce();
  });
});

describe('agentBox の確かめ方', () => {
  const when = (ms: number) => `T${ms}`;
  it('枠の中には、ボタンを置かない（畳む操作は、見出しの行）。「いまの差分を見る」は、渡したときだけ', () => {
    expect(agentBox(REPORT, when)!.querySelector('button')).toBeNull();
    const onDiff = vi.fn();
    const el = agentBox(REPORT, when, onDiff)!;
    const link = [...el.querySelectorAll('button')];
    expect(link.map((b) => b.textContent)).toEqual(['いまの差分を見る']);
    (link[0] as HTMLElement).click();
    expect(onDiff).toHaveBeenCalledOnce();
    // 「対応しなかった」の申告にも、付く（何が変わったかは、申告の中身に関わらず、見られる）
    expect(agentBox({ ...REPORT, verdict: 'skipped' }, when, onDiff)!.querySelector('button.difflink')).not.toBeNull();
  });
  it('「対応した」には、確かめ方（もう一度レビュー）を添える。「対応しなかった」には、添えない', () => {
    expect(agentBox(REPORT, when)!.textContent).toContain('確かめるには、もう一度レビューを実行してください（次のレビューに、解消したかが出ます）。');
    expect(agentBox({ ...REPORT, verdict: 'skipped' }, when)!.textContent).not.toContain('確かめるには');
  });
});

// ── 申告の状態（対応にした指摘ごとに、申告が保存されているか）──

const NOTE = (done_at: number | null): Note => ({ memo: '', memo_at: null, done_at, done_head: null, triage_at: null, triage_head: null, triage_source: null });
const R = (verdict: 'did' | 'skipped' = 'did', source = 'cli:claude-code'): AgentReport => ({ verdict, text: 'x', reported_at: 1, source });

describe('agentStatus（対応にした指摘ごとに、申告があるか）', () => {
  it('数えるのは、「対応」の指摘だけ。待ち・無視・未仕分けは、報告を求めない', () => {
    expect(agentStatus(['wait', 'dismiss', 'new'], [], [])).toEqual({ total: 0, reported: 0, missing: 0, skipped: 0 });
    expect(agentStatus(['fix', 'wait', 'fix', 'new'], [], [null, R(), null, null])).toEqual({ total: 2, reported: 0, missing: 2, skipped: 0 });
  });
  it('「対応した」も「対応しなかった」も、報告済みに数える。「対応しなかった」は、別に数える（あなたの判断が、まだ残っている）', () => {
    expect(agentStatus(['fix', 'fix'], [], [R('did'), R('skipped')])).toEqual({ total: 2, reported: 2, missing: 0, skipped: 1 });
    expect(agentStatus(['fix', 'fix'], [], [R('did'), R('did')])).toEqual({ total: 2, reported: 2, missing: 0, skipped: 0 });
  });
  it('「対応しなかった」の申告でも、仕分けを「対応」から変えたものは、数えない（あなたが、もう判断した）', () => {
    expect(agentStatus(['wait', 'dismiss', 'fix'], [], [R('skipped'), R('skipped'), R('did')])).toEqual({ total: 1, reported: 1, missing: 0, skipped: 0 });
  });
  it('あなたが「対応済み」にした指摘は、報告を求めない（数に入れない。「対応しなかった」でも、数えない）', () => {
    expect(agentStatus(['fix', 'fix', 'fix'], [NOTE(5), NOTE(null), NOTE(null)], [null, R(), null])).toEqual({ total: 2, reported: 1, missing: 1, skipped: 0 });
    expect(agentStatus(['fix', 'fix'], [NOTE(5), NOTE(null)], [R('skipped'), R('did')])).toEqual({ total: 1, reported: 1, missing: 0, skipped: 0 });
  });
  it('一部だけ報告されたとき（抜け・漏れ）は、足りない件数が分かる', () => {
    expect(agentStatus(['fix', 'fix', 'fix', 'fix', 'fix'], [], [R(), R(), R('skipped'), null, null])).toEqual({ total: 5, reported: 3, missing: 2, skipped: 1 });
  });
  it('申告や記録の一覧が、短くても、未定義でも、落ちない（「無い」として扱う）', () => {
    expect(agentStatus(['fix', 'fix'], undefined, undefined)).toEqual({ total: 2, reported: 0, missing: 2, skipped: 0 });
    expect(agentStatus(['fix', 'fix'], [NOTE(null)], [R()])).toEqual({ total: 2, reported: 1, missing: 1, skipped: 0 });
    expect(agentStatus([], [], [])).toEqual({ total: 0, reported: 0, missing: 0, skipped: 0 });
  });
});

describe('agentStatusBar（状態と、取り込みのボタン。ボタンは、足りないときだけ）', () => {
  it('対応の指摘が 0 件なら、何も出さない', () => {
    expect(agentStatusBar({ total: 0, reported: 0, missing: 0, skipped: 0 }, ui(), acts())).toBeNull();
  });
  it('**全部に申告があっても、「対応しなかった」があれば、あなたの判断が残っていると言う**（完了したように見せない）。ボタンは、出さない', () => {
    const el = agentStatusBar({ total: 4, reported: 4, missing: 0, skipped: 1 }, ui(), acts())!;
    expect(el.querySelector('.agentstate')!.textContent).toBe('対応内容: 保存済み（4/4）・対応しなかった 1 件（理由を読んで、仕分けるか、対応済みにしてください）');
    // 「無視」「待ち」だけでなく、「対応済みにする」も選べると分かる（誤検知で、中身はすでに満たされているとき、「対応済み」が、実態に合う）
    expect(el.querySelector('.agentstate')!.textContent).toContain('対応済みにしてください');
    expect(el.querySelector('.agentstate')!.className).toContain('decide');
    expect(el.querySelector('.agentstate')!.className, '「保存済み」の緑ではなく、判断が残っている印').not.toContain('ok');
    expect(el.querySelectorAll('button'), '取り込むものは無い').toHaveLength(0);
  });
  it('一部だけで、「対応しなかった」もあるときは、両方を言って、取り込みのボタンも出す', () => {
    const el = agentStatusBar({ total: 5, reported: 3, missing: 2, skipped: 1 }, ui(), acts())!;
    expect(el.querySelector('.agentstate')!.textContent).toBe('対応内容: 保存済み 3/5 ・未報告 2 件・対応しなかった 1 件（理由を読んで、仕分けるか、対応済みにしてください）');
    expect(btn(el, '対応内容を取り込む')).toBeDefined();
  });
  it('「対応しなかった」が無ければ、判断の手がかりは、添えない', () => {
    expect(agentStatusBar({ total: 5, reported: 5, missing: 0, skipped: 0 }, ui(), acts())!.textContent).not.toContain('対応済みにしてください');
    expect(agentStatusBar({ total: 3, reported: 1, missing: 2, skipped: 0 }, ui(), acts())!.textContent).not.toContain('対応しなかった');
  });
  it('全部に申告があれば、「保存済み」と出して、ボタンは出さない', () => {
    const el = agentStatusBar({ total: 5, reported: 5, missing: 0, skipped: 0 }, ui(), acts())!;
    expect(el.textContent).toBe('対応内容: 保存済み（5/5）');
    expect(el.querySelector('.agentstate')!.className).toContain('ok');
    expect(btn(el, '対応内容を取り込む')).toBeUndefined();
    expect(el.querySelectorAll('button')).toHaveLength(0);
  });
  it('一部だけなら、保存済みの件数と、未報告の件数を言って、取り込みのボタンを出す', () => {
    const a = acts();
    const el = agentStatusBar({ total: 5, reported: 3, missing: 2, skipped: 0 }, ui(), a)!;
    expect(el.querySelector('.agentstate')!.textContent).toBe('対応内容: 保存済み 3/5 ・未報告 2 件');
    expect(el.querySelector('.agentstate')!.className).toContain('missing');
    btn(el, '対応内容を取り込む')!.click();
    expect(a.fetch).toHaveBeenCalledExactlyOnceWith(false);
  });
  it('1 件も無ければ、「未報告 N 件」と言って、ボタンを出す', () => {
    const el = agentStatusBar({ total: 3, reported: 0, missing: 3, skipped: 0 }, ui(), acts())!;
    expect(el.querySelector('.agentstate')!.textContent).toBe('対応内容: 未報告 3 件');
    expect(btn(el, '対応内容を取り込む')).toBeDefined();
  });
  it('取り込み中は、ボタンを押せない', () => {
    const el = agentStatusBar({ total: 2, reported: 0, missing: 2, skipped: 0 }, ui({ busy: true }), acts())!;
    expect(btn(el, '取り込んでいます…')!.disabled).toBe(true);
  });
});

describe('申告の保存元', () => {
  it('cli: は mrcr handoff report、mcp: は MCP、それ以外は会話の記録（ログ）', () => {
    expect(agentSourceKind('cli:claude-code')).toBe('cli');
    expect(agentSourceKind('mcp:x')).toBe('mcp');
    expect(agentSourceKind('sess1')).toBe('log');
    expect(agentSourceKind('')).toBe('log');
    expect(agentSourceLabel('cli:claude-code')).toBe('claude-code が、mrcr handoff report で保存');
    expect(agentSourceLabel('mcp:x')).toBe('x が、MCP で保存');
    expect(agentSourceLabel('sess1')).toBe('ログから取り込み');
  });
  it('カードの申告は、CLI で保存したものに、「ログから取り込み」と言わない', () => {
    const when = (ms: number) => `T${ms}`;
    const cli = agentBox(R('did', 'cli:claude-code'), when)!.textContent!;
    expect(cli).toContain('claude-code が、mrcr handoff report で保存');
    expect(cli).not.toContain('ログから取り込み');
    expect(agentBox(R('did', 'sess1'), when)!.textContent).toContain('ログから取り込み');
  });
});

describe('agentSummary（保存済みの申告を、無かったことにしない）', () => {
  it('ログから見つからなくても、すでに保存されている申告があれば、そう言う', () => {
    const t = agentSummary({ ...FETCH, found: 0, updated: 0, agent_reports: [R(), null, R('skipped')] });
    expect(t).toContain('会話の記録からは、報告は見つかりませんでした');
    expect(t).toContain('すでに保存されている申告 2 件は、そのままです。');
  });
  it('保存済みの申告が無ければ、言わない。取り込めたときも、言わない', () => {
    expect(agentSummary({ ...FETCH, found: 0, updated: 0, agent_reports: [null, null] })).not.toContain('すでに保存されている');
    expect(agentSummary({ ...FETCH, agent_reports: [R()] })).not.toContain('すでに保存されている');
  });
});

describe('refreshAgentReportsOnFocus（窓に戻ったとき、申告だけを読み直す）', () => {
  const REVIEW = (agent_reports: (AgentReport | null)[]) => ({ id: 'r1', agent_reports });
  const make = (over: Partial<AgentRefreshDeps> = {}) => {
    const apply = vi.fn();
    const load = vi.fn(async () => [R(), null]);
    const deps: AgentRefreshDeps = {
      current: () => REVIEW([null, null]),
      busy: () => false,
      now: () => 10_000,
      load,
      apply,
      gate: { at: 0 },
      ...over,
    };
    return { deps, apply, load };
  };

  it('申告が変わっていたら、読み直した値を渡す（true）', async () => {
    const { deps, apply, load } = make();
    expect(await refreshAgentReportsOnFocus(deps)).toBe(true);
    expect(load).toHaveBeenCalledExactlyOnceWith('r1');
    expect(apply).toHaveBeenCalledExactlyOnceWith('r1', [R(), null]);
  });
  it('変わっていなければ、何もしない（描き直さない）', async () => {
    const { deps, apply } = make({ current: () => REVIEW([R(), null]) });
    expect(await refreshAgentReportsOnFocus(deps)).toBe(false);
    expect(apply).not.toHaveBeenCalled();
  });
  it('レビューを開いていない・取り込み中やレビュー実行中は、読まない', async () => {
    const a = make({ current: () => null });
    expect(await refreshAgentReportsOnFocus(a.deps)).toBe(false);
    expect(a.load).not.toHaveBeenCalled();
    const b = make({ busy: () => true });
    expect(await refreshAgentReportsOnFocus(b.deps)).toBe(false);
    expect(b.load).not.toHaveBeenCalled();
  });
  it('続けて読まない（間隔の内は、読まない）。間隔が過ぎたら、また読む', async () => {
    let t = 10_000;
    const { deps, load } = make({ now: () => t });
    await refreshAgentReportsOnFocus(deps);
    t = 10_500;
    expect(await refreshAgentReportsOnFocus(deps)).toBe(false);
    expect(load).toHaveBeenCalledOnce();
    t = 12_001;
    await refreshAgentReportsOnFocus(deps);
    expect(load).toHaveBeenCalledTimes(2);
  });
  it('読んでいる間に、別のレビューへ移っていたら、捨てる', async () => {
    let cur = REVIEW([null, null]);
    const { deps, apply } = make({
      current: () => cur,
      load: async () => {
        cur = { id: 'r2', agent_reports: [null, null] }; // 読んでいる間に、移った
        return [R(), null];
      },
    });
    expect(await refreshAgentReportsOnFocus(deps)).toBe(false);
    expect(apply).not.toHaveBeenCalled();
  });
  it('読めなかったとき（失敗・レビューが無い）は、いまの表示を消さない', async () => {
    const failing = make({ load: async () => Promise.reject(new Error('DB が開けない')) });
    expect(await refreshAgentReportsOnFocus(failing.deps)).toBe(false);
    expect(failing.apply).not.toHaveBeenCalled();
    const none = make({ load: async () => null });
    expect(await refreshAgentReportsOnFocus(none.deps)).toBe(false);
    expect(none.apply).not.toHaveBeenCalled();
  });
  it('間隔は、指定できる', async () => {
    const { deps, load } = make({ minIntervalMs: 0, now: () => 5 });
    await refreshAgentReportsOnFocus(deps);
    await refreshAgentReportsOnFocus(deps);
    expect(load).toHaveBeenCalledTimes(2);
  });
});
