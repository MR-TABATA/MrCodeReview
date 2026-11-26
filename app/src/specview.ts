import type { Messages } from './locales';
import type { SectionMeta, SpecParam } from './types';

/** 貼り付けた文書、または 1 つのファイル（の節）の文字数の上限。合計の上限は、実行のときにバックエンドが確かめる */
export const MAX_SPEC_CHARS = 30_000;
/** ファイルで指定できる設計書の件数の上限（バックエンドの `MAX_SPEC_FILES` と同じ） */
export const MAX_SPEC_FILES = 5;

export interface SpecViewOptions {
  messages: Messages;
  onChange?: (params: SpecParam[], valid: boolean) => void;
  onFileSelect?: (filePath: string) => Promise<SectionMeta[]>;
  onPickFile?: () => Promise<string | null>;
}

/** 指定した設計書ファイル 1 件。節は、ファイルごとに選ぶ */
interface SpecItem {
  path: string;
  section: string;
  sections: SectionMeta[];
}

export class SpecView {
  public readonly root: HTMLElement;
  private messages: Messages;
  private readonly onChange?: (params: SpecParam[], valid: boolean) => void;
  private readonly onFileSelect?: (filePath: string) => Promise<SectionMeta[]>;
  private readonly onPickFile?: () => Promise<string | null>;

  private enabled: boolean = false;
  private disabled: boolean = false;
  private mode: 'file' | 'paste' = 'file';
  private files: string[] = [];
  private items: SpecItem[] = [];
  private pasteText: string = '';

  private readonly toggleCheckbox: HTMLInputElement;
  private readonly cardTitle: HTMLElement;
  private readonly container: HTMLElement;
  private readonly tabFileBtn: HTMLButtonElement;
  private readonly tabPasteBtn: HTMLButtonElement;
  private readonly filePanel: HTMLElement;
  private readonly pastePanel: HTMLElement;
  private readonly pickFileBtn: HTMLButtonElement;
  private readonly clearFileBtn: HTMLButtonElement;
  private readonly clearPasteBtn: HTMLButtonElement;
  private readonly filePathInput: HTMLInputElement;
  private readonly fileDatalist: HTMLDataListElement;
  private readonly fileList: HTMLElement;
  private readonly fileCount: HTMLElement;
  private readonly textarea: HTMLTextAreaElement;
  private readonly charCounter: HTMLElement;
  /** 上限に達したときの知らせを出しているか（次の変更で消す） */
  private limitNotice = false;

  constructor(options: SpecViewOptions) {
    this.messages = options.messages;
    this.onChange = options.onChange;
    this.onFileSelect = options.onFileSelect;
    this.onPickFile = options.onPickFile;

    this.root = document.createElement('div');
    this.root.className = 'spec-view-root';
    this.root.style.display = 'none';

    // 内部状態用の隠しチェックボックス（後方互換用）
    this.toggleCheckbox = document.createElement('input');
    this.toggleCheckbox.type = 'checkbox';
    this.toggleCheckbox.className = 'spec-toggle-checkbox';
    this.toggleCheckbox.checked = false;
    this.toggleCheckbox.style.display = 'none';
    this.toggleCheckbox.addEventListener('change', () => {
      this.enabled = this.toggleCheckbox.checked;
      this.updateVisibility();
      this.notifyChange();
    });
    this.root.appendChild(this.toggleCheckbox);

    // ヘッダー行: タイトル（左）とタブ切り替え（右）
    const headerRow = document.createElement('div');
    headerRow.className = 'spec-card-header';

    this.cardTitle = document.createElement('span');
    this.cardTitle.className = 'spec-card-title';
    this.cardTitle.textContent = this.messages.specView.cardTitle;
    headerRow.appendChild(this.cardTitle);

    // タブバー
    const tabBar = document.createElement('div');
    tabBar.className = 'spec-tab-bar';

    this.tabFileBtn = document.createElement('button');
    this.tabFileBtn.type = 'button';
    this.tabFileBtn.className = 'spec-tab-btn active';
    this.tabFileBtn.textContent = this.messages.specView.tabFile;
    this.tabFileBtn.addEventListener('click', () => this.setMode('file'));

    this.tabPasteBtn = document.createElement('button');
    this.tabPasteBtn.type = 'button';
    this.tabPasteBtn.className = 'spec-tab-btn';
    this.tabPasteBtn.textContent = this.messages.specView.tabPaste;
    this.tabPasteBtn.addEventListener('click', () => this.setMode('paste'));

    tabBar.appendChild(this.tabFileBtn);
    tabBar.appendChild(this.tabPasteBtn);
    headerRow.appendChild(tabBar);
    this.root.appendChild(headerRow);

    // パネルコンテナ
    this.container = document.createElement('div');
    this.container.className = 'spec-container';
    this.container.style.display = 'none';

    // 1. ファイル選択パネル（追加の行 ＋ 指定済みの一覧）
    this.filePanel = document.createElement('div');
    this.filePanel.className = 'spec-panel spec-file-panel';

    const addRow = document.createElement('div');
    addRow.className = 'spec-file-add-row';

    // ファイル選択ダイアログ呼び出しボタン
    this.pickFileBtn = document.createElement('button');
    this.pickFileBtn.type = 'button';
    this.pickFileBtn.className = 'spec-file-btn';
    this.pickFileBtn.textContent = this.messages.specView.pickFile;
    this.pickFileBtn.title = this.messages.specView.pickFileTitle;
    this.pickFileBtn.addEventListener('click', async () => {
      if (this.onPickFile) {
        const picked = await this.onPickFile();
        if (picked) {
          await this.addFile(picked);
        }
      }
    });

    // パス直接入力欄（datalist でリポジトリ内候補もサジェスト）。確定（Enter・候補の選択）で一覧に足す
    const datalistId = `spec-files-${Math.random().toString(36).slice(2, 8)}`;
    this.fileDatalist = document.createElement('datalist');
    this.fileDatalist.id = datalistId;

    this.filePathInput = document.createElement('input');
    this.filePathInput.type = 'text';
    this.filePathInput.className = 'spec-file-path-display';
    this.filePathInput.placeholder = this.messages.specView.filePathPlaceholder;
    this.filePathInput.setAttribute('list', datalistId);
    this.filePathInput.addEventListener('change', async () => {
      const v = this.filePathInput.value;
      this.filePathInput.value = '';
      await this.addFile(v);
    });

    const clearFileBtn = document.createElement('button');
    clearFileBtn.type = 'button';
    clearFileBtn.className = 'spec-clear-btn spec-file-clear-btn';
    clearFileBtn.textContent = this.messages.specView.clearButton;
    clearFileBtn.title = this.messages.specView.clearSelectionTitle;
    this.clearFileBtn = clearFileBtn;
    clearFileBtn.addEventListener('click', () => {
      this.clearFile();
    });

    addRow.appendChild(this.pickFileBtn);
    addRow.appendChild(this.filePathInput);
    addRow.appendChild(this.fileDatalist);
    addRow.appendChild(clearFileBtn);
    this.filePanel.appendChild(addRow);

    this.fileList = document.createElement('div');
    this.fileList.className = 'spec-file-list';
    this.filePanel.appendChild(this.fileList);

    this.fileCount = document.createElement('div');
    this.fileCount.className = 'spec-file-count';
    this.filePanel.appendChild(this.fileCount);
    this.container.appendChild(this.filePanel);

    // 2. 貼り付けパネル（貼り付けは 1 件）
    this.pastePanel = document.createElement('div');
    this.pastePanel.className = 'spec-panel spec-paste-panel';
    this.pastePanel.style.display = 'none';

    this.textarea = document.createElement('textarea');
    this.textarea.className = 'spec-textarea';
    this.textarea.placeholder = this.messages.specView.pasteAreaPlaceholder;
    this.textarea.rows = 4;
    this.textarea.addEventListener('input', () => {
      this.pasteText = this.textarea.value;
      this.updateCharCounter();
      this.notifyChange();
    });

    const pasteFooter = document.createElement('div');
    pasteFooter.className = 'spec-paste-footer';

    this.charCounter = document.createElement('span');
    this.charCounter.className = 'spec-char-counter';

    const clearPasteBtn = document.createElement('button');
    clearPasteBtn.type = 'button';
    clearPasteBtn.className = 'spec-clear-btn spec-paste-clear-btn';
    clearPasteBtn.textContent = this.messages.specView.clearButton;
    clearPasteBtn.title = this.messages.specView.clearInputTitle;
    this.clearPasteBtn = clearPasteBtn;
    clearPasteBtn.addEventListener('click', () => {
      this.pasteText = '';
      this.textarea.value = '';
      this.updateCharCounter();
      this.notifyChange();
    });

    pasteFooter.appendChild(this.charCounter);
    pasteFooter.appendChild(clearPasteBtn);

    this.pastePanel.appendChild(this.textarea);
    this.pastePanel.appendChild(pasteFooter);
    this.container.appendChild(this.pastePanel);

    this.root.appendChild(this.container);

    this.renderFileOptions();
    this.renderItems();
    this.updateCharCounter();
  }

  public setEnabled(enabled: boolean): void {
    this.enabled = enabled;
    this.toggleCheckbox.checked = enabled;
    this.updateVisibility();
    this.notifyChange();
  }

  public isEnabled(): boolean {
    return this.enabled;
  }

  /** リポジトリ内の候補。一覧に入っていないファイル（別のリポジトリのもの）は、外す */
  public setFiles(files: string[]): void {
    this.files = [...files];
    this.renderFileOptions();
    const kept = this.items.filter((it) => this.files.includes(it.path));
    if (kept.length !== this.items.length) {
      this.items = kept;
      this.renderItems();
    }
    this.notifyChange();
  }

  public setMode(mode: 'file' | 'paste'): void {
    this.mode = mode;
    if (mode === 'file') {
      this.tabFileBtn.classList.add('active');
      this.tabPasteBtn.classList.remove('active');
      this.filePanel.style.display = '';
      this.pastePanel.style.display = 'none';
    } else {
      this.tabFileBtn.classList.remove('active');
      this.tabPasteBtn.classList.add('active');
      this.filePanel.style.display = 'none';
      this.pastePanel.style.display = '';
    }
    this.notifyChange();
  }

  /** 実行に渡す、基準の文書の指定（指定した順）。ファイルのときは複数、貼り付けのときは 1 件 */
  public getParams(): SpecParam[] {
    if (!this.enabled) {
      return [];
    }
    if (this.mode === 'file') {
      return this.items.map((it) => ({
        source_type: 'file' as const,
        file_path: it.path,
        section: it.section || null,
      }));
    }
    const trimmed = this.pasteText.trim();
    return trimmed ? [{ source_type: 'paste' as const, paste_text: trimmed }] : [];
  }

  public isValid(): boolean {
    if (!this.enabled) {
      return true;
    }
    if (this.mode === 'file') {
      return this.items.length >= 1 && this.items.length <= MAX_SPEC_FILES;
    }
    const chars = this.pasteText.trim().length;
    return chars > 0 && chars <= MAX_SPEC_CHARS;
  }

  /** ファイルを 1 件、一覧に足す。同じファイル（節を選ぶ前のもの）が既にあるときと、上限のときは、足さない */
  public async addFile(filePath: string): Promise<void> {
    const path = filePath.trim();
    if (!path) {
      return;
    }
    if (this.items.some((it) => it.path === path && !it.section)) {
      return;
    }
    if (this.items.length >= MAX_SPEC_FILES) {
      this.limitNotice = true;
      this.renderCount();
      return;
    }
    this.limitNotice = false;
    const item: SpecItem = { path, section: '', sections: [] };
    this.items.push(item);
    this.renderItems();
    this.notifyChange();
    if (this.onFileSelect) {
      try {
        item.sections = (await this.onFileSelect(path)) ?? [];
      } catch (e) {
        console.error('Failed to load sections for spec file', e);
        item.sections = [];
      }
      // 読んでいるあいだに、外されていたら、何もしない
      if (this.items.includes(item)) {
        this.renderItems();
      }
    }
  }

  public removeFile(index: number): void {
    if (index < 0 || index >= this.items.length) {
      return;
    }
    this.items.splice(index, 1);
    this.limitNotice = false;
    this.renderItems();
    this.notifyChange();
  }

  public setSection(index: number, section: string): void {
    const it = this.items[index];
    if (!it) {
      return;
    }
    it.section = section;
    this.notifyChange();
  }

  public clearFile(): void {
    this.items = [];
    this.limitNotice = false;
    this.filePathInput.value = '';
    this.renderItems();
    this.notifyChange();
  }

  public setDisabled(disabled: boolean): void {
    this.disabled = disabled;
    this.toggleCheckbox.disabled = disabled;
    this.tabFileBtn.disabled = disabled;
    this.tabPasteBtn.disabled = disabled;
    this.pickFileBtn.disabled = disabled;
    this.filePathInput.disabled = disabled;
    this.clearFileBtn.disabled = disabled;
    this.textarea.disabled = disabled;
    this.renderItems();
  }

  public clear(): void {
    this.clearFile();
    this.pasteText = '';
    this.textarea.value = '';
    this.updateCharCounter();
    this.notifyChange();
  }

  public setMessages(messages: Messages): void {
    this.messages = messages;
    this.cardTitle.textContent = this.messages.specView.cardTitle;
    this.tabFileBtn.textContent = this.messages.specView.tabFile;
    this.tabPasteBtn.textContent = this.messages.specView.tabPaste;
    this.pickFileBtn.textContent = this.messages.specView.pickFile;
    this.pickFileBtn.title = this.messages.specView.pickFileTitle;
    this.filePathInput.placeholder = this.messages.specView.filePathPlaceholder;
    this.clearFileBtn.textContent = this.messages.specView.clearButton;
    this.clearFileBtn.title = this.messages.specView.clearSelectionTitle;
    this.textarea.placeholder = this.messages.specView.pasteAreaPlaceholder;
    this.clearPasteBtn.textContent = this.messages.specView.clearButton;
    this.clearPasteBtn.title = this.messages.specView.clearInputTitle;
    this.renderFileOptions();
    this.renderItems();
    this.updateCharCounter();
  }

  private updateVisibility(): void {
    this.root.style.display = this.enabled ? '' : 'none';
    this.container.style.display = this.enabled ? '' : 'none';
  }

  private renderFileOptions(): void {
    this.fileDatalist.innerHTML = '';
    for (const f of this.files) {
      const opt = document.createElement('option');
      opt.value = f;
      this.fileDatalist.appendChild(opt);
    }
  }

  /** 指定済みの一覧を描き直す（1 行 = ファイル名・節の選択・外すボタン） */
  private renderItems(): void {
    this.fileList.innerHTML = '';
    this.items.forEach((it, index) => {
      const row = document.createElement('div');
      row.className = 'spec-file-item';

      const name = document.createElement('span');
      name.className = 'spec-file-item-name';
      name.textContent = it.path;
      name.title = it.path;
      row.appendChild(name);

      if (it.sections.length > 0) {
        const select = document.createElement('select');
        select.className = 'spec-section-select';
        select.disabled = this.disabled;
        const whole = document.createElement('option');
        whole.value = '';
        whole.textContent = this.messages.specView.allSections;
        select.appendChild(whole);
        for (const sec of it.sections) {
          const opt = document.createElement('option');
          opt.value = sec.title;
          opt.textContent = `${'　'.repeat(Math.max(0, sec.level - 1))}${sec.title}`;
          select.appendChild(opt);
        }
        select.value = it.section;
        select.addEventListener('change', () => this.setSection(index, select.value));
        row.appendChild(select);
      }

      const remove = document.createElement('button');
      remove.type = 'button';
      remove.className = 'spec-clear-btn spec-file-remove-btn';
      remove.textContent = this.messages.specView.removeFile;
      remove.disabled = this.disabled;
      remove.addEventListener('click', () => this.removeFile(index));
      row.appendChild(remove);

      this.fileList.appendChild(row);
    });
    this.renderCount();
  }

  private renderCount(): void {
    if (this.limitNotice) {
      this.fileCount.textContent = this.messages.specView.maxFilesReached(MAX_SPEC_FILES);
      this.fileCount.classList.add('over-limit');
    } else {
      this.fileCount.textContent = this.items.length > 0 ? this.messages.specView.addedCount(this.items.length, MAX_SPEC_FILES) : '';
      this.fileCount.classList.remove('over-limit');
    }
  }

  private updateCharCounter(): void {
    const chars = this.pasteText.length;
    const estTokens = Math.ceil(chars / 2);
    if (chars > MAX_SPEC_CHARS) {
      this.charCounter.textContent = this.messages.specView.charCountOver(chars, MAX_SPEC_CHARS);
      this.charCounter.classList.add('over-limit');
    } else {
      this.charCounter.textContent = this.messages.specView.charCount(chars, estTokens);
      this.charCounter.classList.remove('over-limit');
    }
  }

  private notifyChange(): void {
    if (this.onChange) {
      this.onChange(this.getParams(), this.isValid());
    }
  }
}
