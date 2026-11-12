// @vitest-environment jsdom
import { describe, it, expect, vi } from 'vitest';
import { SpecView, MAX_SPEC_CHARS } from './specview';
import { MESSAGES } from './locales';

describe('SpecView', () => {
  const messages = MESSAGES.ja;

  it('initializes as disabled and hidden', () => {
    const view = new SpecView({ messages });
    expect(view.isEnabled()).toBe(false);
    expect(view.getParam()).toBeNull();
    expect(view.isValid()).toBe(true);
    const container = view.root.querySelector('.spec-container') as HTMLElement;
    expect(container.style.display).toBe('none');
  });

  it('enabling changes visibility and valid state', () => {
    let currentParam: any = null;
    let isValid = false;

    const view = new SpecView({
      messages,
      onChange: (p, valid) => {
        currentParam = p;
        isValid = valid;
      },
    });

    view.setEnabled(true);
    expect(view.isEnabled()).toBe(true);
    // 初期状態ではファイルが未選択なので invalid
    expect(isValid).toBe(false);
    expect(currentParam).toBeNull();

    const container = view.root.querySelector('.spec-container') as HTMLElement;
    expect(container.style.display).toBe('');
  });

  it('selects file and fetches sections', async () => {
    const onFileSelect = vi.fn().mockResolvedValue([
      { level: 1, title: '仕様書', line_start: 1 },
      { level: 2, title: '認証節', line_start: 5 },
    ]);

    let currentParam: any = null;
    const view = new SpecView({
      messages,
      onFileSelect,
      onChange: (p) => {
        currentParam = p;
      },
    });

    view.setEnabled(true);
    view.setFiles(['docs/auth.md', 'README.md']);

    const fileSelect = view.root.querySelector('.spec-file-select') as HTMLSelectElement;
    fileSelect.value = 'docs/auth.md';
    fileSelect.dispatchEvent(new Event('change'));

    // 非同期の onFileSelect 完了を待つ
    await new Promise((r) => setTimeout(r, 0));

    expect(onFileSelect).toHaveBeenCalledWith('docs/auth.md');
    expect(view.isValid()).toBe(true);
    expect(currentParam).toEqual({
      source_type: 'file',
      file_path: 'docs/auth.md',
      section: null,
    });

    // 節を選択
    const sectionSelect = view.root.querySelector('.spec-section-select') as HTMLSelectElement;
    expect(sectionSelect.disabled).toBe(false);
    sectionSelect.value = '認証節';
    sectionSelect.dispatchEvent(new Event('change'));

    expect(currentParam).toEqual({
      source_type: 'file',
      file_path: 'docs/auth.md',
      section: '認証節',
    });
  });

  it('paste mode validates length and enforces upper limit', () => {
    let isValid = false;
    let currentParam: any = null;

    const view = new SpecView({
      messages,
      onChange: (p, valid) => {
        currentParam = p;
        isValid = valid;
      },
    });

    view.setEnabled(true);
    view.setMode('paste');

    const textarea = view.root.querySelector('.spec-textarea') as HTMLTextAreaElement;
    const counter = view.root.querySelector('.spec-char-counter') as HTMLElement;

    // 空の入力
    expect(isValid).toBe(false);
    expect(currentParam).toBeNull();

    // 正常なテキスト入力
    textarea.value = '要件仕様テキスト';
    textarea.dispatchEvent(new Event('input'));
    expect(isValid).toBe(true);
    expect(currentParam).toEqual({
      source_type: 'paste',
      paste_text: '要件仕様テキスト',
    });
    expect(counter.textContent).toContain('8 文字');

    // 上限超過
    textarea.value = 'a'.repeat(MAX_SPEC_CHARS + 1);
    textarea.dispatchEvent(new Event('input'));
    expect(isValid).toBe(false);
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
});
