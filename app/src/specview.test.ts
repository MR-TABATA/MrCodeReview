// @vitest-environment jsdom
import { describe, it, expect, vi } from 'vitest';
import { SpecView, MAX_SPEC_CHARS, MAX_SPEC_FILES } from './specview';
import { MESSAGES } from './locales';
import type { SpecParam } from './types';

const messages = MESSAGES.ja;
const flush = () => new Promise((r) => setTimeout(r, 0));

/** 変更の通知を溜めておく部品付きで作る */
function make(opts: { onFileSelect?: (p: string) => Promise<any> } = {}) {
  const seen: { params: SpecParam[]; valid: boolean } = { params: [], valid: true };
  const view = new SpecView({
    messages,
    onFileSelect: opts.onFileSelect,
    onChange: (params, valid) => {
      seen.params = params;
      seen.valid = valid;
    },
  });
  return { view, seen };
}

const names = (view: SpecView) => Array.from(view.root.querySelectorAll('.spec-file-item-name')).map((e) => e.textContent);

describe('SpecView', () => {
  it('initializes as disabled and hidden', () => {
    const view = new SpecView({ messages });
    expect(view.isEnabled()).toBe(false);
    expect(view.getParams()).toEqual([]);
    expect(view.isValid()).toBe(true);
    const container = view.root.querySelector('.spec-container') as HTMLElement;
    expect(container.style.display).toBe('none');
  });

  it('enabling changes visibility and valid state', () => {
    const { view, seen } = make();
    view.setEnabled(true);
    expect(view.isEnabled()).toBe(true);
    // 初期状態ではファイルが未選択なので invalid
    expect(seen.valid).toBe(false);
    expect(seen.params).toEqual([]);
    const container = view.root.querySelector('.spec-container') as HTMLElement;
    expect(container.style.display).toBe('');
  });

  it('adds a file, fetches its sections and lets you pick a section per file', async () => {
    const onFileSelect = vi.fn().mockResolvedValue([
      { level: 1, title: '仕様書', line_start: 1 },
      { level: 2, title: '認証節', line_start: 5 },
    ]);
    const { view, seen } = make({ onFileSelect });
    view.setEnabled(true);
    view.setFiles(['docs/auth.md', 'README.md']);

    await view.addFile('docs/auth.md');
    await flush();

    expect(onFileSelect).toHaveBeenCalledWith('docs/auth.md');
    expect(view.isValid()).toBe(true);
    expect(seen.params).toEqual([{ source_type: 'file', file_path: 'docs/auth.md', section: null }]);

    const sectionSelect = view.root.querySelector('.spec-section-select') as HTMLSelectElement;
    expect(sectionSelect.disabled).toBe(false);
    sectionSelect.value = '認証節';
    sectionSelect.dispatchEvent(new Event('change'));
    expect(seen.params).toEqual([{ source_type: 'file', file_path: 'docs/auth.md', section: '認証節' }]);
  });

  it('keeps several files in the order they were added, each with its own section', async () => {
    const onFileSelect = vi.fn(async (p: string) => (p === 'docs/design.md' ? [{ level: 2, title: '認証', line_start: 3 }] : []));
    const { view, seen } = make({ onFileSelect });
    view.setEnabled(true);
    view.setFiles(['docs/req.md', 'docs/design.md', 'docs/tasks.md']);

    await view.addFile('docs/design.md');
    await view.addFile('docs/req.md');
    await view.addFile('docs/tasks.md');
    await flush();

    expect(names(view)).toEqual(['docs/design.md', 'docs/req.md', 'docs/tasks.md']);
    // 節の選択肢が出るのは、節のあるファイルだけ。節は、そのファイルにだけ効く
    const selects = view.root.querySelectorAll('.spec-section-select');
    expect(selects.length).toBe(1);
    (selects[0] as HTMLSelectElement).value = '認証';
    selects[0].dispatchEvent(new Event('change'));
    expect(seen.params).toEqual([
      { source_type: 'file', file_path: 'docs/design.md', section: '認証' },
      { source_type: 'file', file_path: 'docs/req.md', section: null },
      { source_type: 'file', file_path: 'docs/tasks.md', section: null },
    ]);
    expect((view.root.querySelector('.spec-file-count') as HTMLElement).textContent).toBe('3 / 5 件を指定中');
  });

  it('ignores a duplicate or empty path and stops at the file limit with a notice', async () => {
    const { view, seen } = make();
    view.setEnabled(true);
    await view.addFile('docs/a.md');
    await view.addFile(' docs/a.md ');
    await view.addFile('   ');
    expect(names(view)).toEqual(['docs/a.md']);

    for (let i = 1; i < MAX_SPEC_FILES; i++) await view.addFile(`docs/f${i}.md`);
    expect(names(view).length).toBe(MAX_SPEC_FILES);
    expect(seen.valid).toBe(true);

    await view.addFile('docs/over.md');
    expect(names(view).length).toBe(MAX_SPEC_FILES);
    expect(names(view)).not.toContain('docs/over.md');
    const count = view.root.querySelector('.spec-file-count') as HTMLElement;
    expect(count.textContent).toContain('最大 5 件');
    expect(count.className).toContain('over-limit');

    // 1 件外すと、また足せて、知らせも消える
    view.removeFile(0);
    expect(count.className).not.toContain('over-limit');
    await view.addFile('docs/over.md');
    expect(names(view)).toContain('docs/over.md');
  });

  it('removes one file (button) and clears all', async () => {
    const { view, seen } = make();
    view.setEnabled(true);
    await view.addFile('docs/a.md');
    await view.addFile('docs/b.md');
    (view.root.querySelectorAll('.spec-file-remove-btn')[0] as HTMLButtonElement).click();
    expect(names(view)).toEqual(['docs/b.md']);
    expect(seen.params.map((p) => p.file_path)).toEqual(['docs/b.md']);

    view.clearFile();
    expect(names(view)).toEqual([]);
    expect(seen.params).toEqual([]);
    expect(seen.valid).toBe(false);
  });

  it('adds a file when a path is typed and confirmed, and empties the input', async () => {
    const { view, seen } = make();
    view.setEnabled(true);
    const input = view.root.querySelector('.spec-file-path-display') as HTMLInputElement;
    input.value = 'docs/typed.md';
    input.dispatchEvent(new Event('change'));
    await flush();
    expect(input.value).toBe('');
    expect(seen.params.map((p) => p.file_path)).toEqual(['docs/typed.md']);
  });

  it('drops files that are not candidates of the repository when the candidates change', async () => {
    const { view } = make();
    view.setEnabled(true);
    view.setFiles(['docs/a.md', 'docs/b.md']);
    await view.addFile('docs/a.md');
    await view.addFile('docs/b.md');
    view.setFiles(['docs/b.md', 'docs/c.md']); // 別のリポジトリに切り替わった
    expect(names(view)).toEqual(['docs/b.md']);
    view.setFiles([]);
    expect(names(view)).toEqual([]);
  });

  it('disables every control, including the per-file ones', async () => {
    const { view } = make({ onFileSelect: async () => [{ level: 1, title: 'x', line_start: 1 }] });
    view.setEnabled(true);
    await view.addFile('docs/a.md');
    await flush();
    view.setDisabled(true);
    expect((view.root.querySelector('.spec-file-btn') as HTMLButtonElement).disabled).toBe(true);
    expect((view.root.querySelector('.spec-file-remove-btn') as HTMLButtonElement).disabled).toBe(true);
    expect((view.root.querySelector('.spec-section-select') as HTMLSelectElement).disabled).toBe(true);
    view.setDisabled(false);
    expect((view.root.querySelector('.spec-file-remove-btn') as HTMLButtonElement).disabled).toBe(false);
  });

  it('paste mode uses only the pasted text (one document) and keeps the file list for later', async () => {
    const { view, seen } = make();
    view.setEnabled(true);
    await view.addFile('docs/a.md');
    view.setMode('paste');
    expect(seen.valid).toBe(false); // 貼り付けが空
    expect(seen.params).toEqual([]);

    const textarea = view.root.querySelector('.spec-textarea') as HTMLTextAreaElement;
    textarea.value = '要件仕様テキスト';
    textarea.dispatchEvent(new Event('input'));
    expect(seen.valid).toBe(true);
    expect(seen.params).toEqual([{ source_type: 'paste', paste_text: '要件仕様テキスト' }]);

    view.setMode('file');
    expect(seen.params.map((p) => p.file_path)).toEqual(['docs/a.md']);
  });

  it('paste mode validates length and enforces upper limit', () => {
    const { view, seen } = make();
    view.setEnabled(true);
    view.setMode('paste');

    const textarea = view.root.querySelector('.spec-textarea') as HTMLTextAreaElement;
    const counter = view.root.querySelector('.spec-char-counter') as HTMLElement;

    // 空の入力
    expect(seen.valid).toBe(false);
    expect(seen.params).toEqual([]);

    // 正常なテキスト入力
    textarea.value = '要件仕様テキスト';
    textarea.dispatchEvent(new Event('input'));
    expect(seen.valid).toBe(true);
    expect(seen.params).toEqual([{ source_type: 'paste', paste_text: '要件仕様テキスト' }]);
    expect(counter.textContent).toContain('8 文字');

    // 上限超過
    textarea.value = 'a'.repeat(MAX_SPEC_CHARS + 1);
    textarea.dispatchEvent(new Event('input'));
    expect(seen.valid).toBe(false);
    expect(counter.className).toContain('over-limit');
    expect(counter.textContent).toContain('上限超過');
  });

  it('clear button resets textarea and validity', () => {
    const view = new SpecView({ messages });
    view.setEnabled(true);
    view.setMode('paste');

    const textarea = view.root.querySelector('.spec-textarea') as HTMLTextAreaElement;
    textarea.value = 'メモ';
    textarea.dispatchEvent(new Event('input'));
    expect(view.isValid()).toBe(true);

    const clearBtn = view.root.querySelector('.spec-paste-panel .spec-clear-btn') as HTMLButtonElement;
    clearBtn.click();

    expect(textarea.value).toBe('');
    expect(view.isValid()).toBe(false);
  });

  it('shows the English texts when the language changes', async () => {
    const { view } = make();
    view.setEnabled(true);
    await view.addFile('docs/a.md');
    view.setMessages(MESSAGES.en);
    expect((view.root.querySelector('.spec-file-count') as HTMLElement).textContent).toBe('1 / 5 files selected');
    expect((view.root.querySelector('.spec-file-remove-btn') as HTMLButtonElement).textContent).toBe('Remove');
  });
});
