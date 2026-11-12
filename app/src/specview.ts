import type { Messages } from './locales';
import type { SectionMeta, SpecParam } from './types';

export const MAX_SPEC_CHARS = 30_000;

export interface SpecViewOptions {
  messages: Messages;
  onChange?: (param: SpecParam | null, valid: boolean) => void;
  onFileSelect?: (filePath: string) => Promise<SectionMeta[]>;
  onPickFile?: () => Promise<string | null>;
}

export class SpecView {
  public readonly root: HTMLElement;
  private messages: Messages;
  private readonly onChange?: (param: SpecParam | null, valid: boolean) => void;
  private readonly onFileSelect?: (filePath: string) => Promise<SectionMeta[]>;
  private readonly onPickFile?: () => Promise<string | null>;

  private enabled: boolean = false;
  private mode: 'file' | 'paste' = 'file';
  private files: string[] = [];
  private selectedFile: string = '';
  private selectedSection: string = '';
  private pasteText: string = '';

  private readonly toggleCheckbox: HTMLInputElement;
  private readonly cardTitle: HTMLElement;
  private readonly container: HTMLElement;
  private readonly tabFileBtn: HTMLButtonElement;
  private readonly tabPasteBtn: HTMLButtonElement;
  private readonly filePanel: HTMLElement;
  private readonly pastePanel: HTMLElement;
  private readonly pickFileBtn: HTMLButtonElement;
  private readonly filePathInput: HTMLInputElement;
  private readonly fileDatalist: HTMLDataListElement;
  private readonly fileSelect: HTMLSelectElement;
  private readonly sectionSelect: HTMLSelectElement;
  private readonly textarea: HTMLTextAreaElement;
  private readonly charCounter: HTMLElement;

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
    this.cardTitle.textContent = '📘 参照する設計書・仕様書の指定';
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

    // 1. ファイル選択パネル
    this.filePanel = document.createElement('div');
    this.filePanel.className = 'spec-panel spec-file-panel';

    // ファイル選択ダイアログ呼び出しボタン
    this.pickFileBtn = document.createElement('button');
    this.pickFileBtn.type = 'button';
    this.pickFileBtn.className = 'spec-file-btn';
    this.pickFileBtn.textContent = '📄 ファイルを選択...';
    this.pickFileBtn.title = 'macOSのファイル選択ダイアログを開きます';
    this.pickFileBtn.addEventListener('click', async () => {
      if (this.onPickFile) {
        const picked = await this.onPickFile();
        if (picked) {
          await this.selectFile(picked);
        }
      }
    });

    // パス直接入力・表示欄（datalistでリポジトリ内候補もサジェスト）
    const datalistId = `spec-files-${Math.random().toString(36).slice(2, 8)}`;
    this.fileDatalist = document.createElement('datalist');
    this.fileDatalist.id = datalistId;

    this.filePathInput = document.createElement('input');
    this.filePathInput.type = 'text';
    this.filePathInput.className = 'spec-file-path-display';
    this.filePathInput.placeholder = 'ファイルパス（例: docs/spec.md）または「ファイルを選択...」ボタン';
    this.filePathInput.setAttribute('list', datalistId);
    this.filePathInput.addEventListener('change', async () => {
      await this.selectFile(this.filePathInput.value);
    });

    // 既存テスト互換用 select（hidden）
    this.fileSelect = document.createElement('select');
    this.fileSelect.className = 'spec-file-select';
    this.fileSelect.style.display = 'none';
    this.fileSelect.addEventListener('change', async () => {
      await this.selectFile(this.fileSelect.value);
    });

    this.sectionSelect = document.createElement('select');
    this.sectionSelect.className = 'spec-section-select';
    this.sectionSelect.disabled = true;
    this.sectionSelect.style.display = 'none';
    this.sectionSelect.addEventListener('change', () => {
      this.selectedSection = this.sectionSelect.value;
      this.notifyChange();
    });

    const clearFileBtn = document.createElement('button');
    clearFileBtn.type = 'button';
    clearFileBtn.className = 'spec-clear-btn spec-file-clear-btn';
    clearFileBtn.textContent = this.messages.specView.clearButton;
    clearFileBtn.title = '選択を解除';
    clearFileBtn.addEventListener('click', () => {
      this.clearFile();
    });

    this.filePanel.appendChild(this.pickFileBtn);
    this.filePanel.appendChild(this.filePathInput);
    this.filePanel.appendChild(this.fileDatalist);
    this.filePanel.appendChild(this.fileSelect);
    this.filePanel.appendChild(this.sectionSelect);
    this.filePanel.appendChild(clearFileBtn);
    this.container.appendChild(this.filePanel);

    // 2. 貼り付けパネル
    this.pastePanel = document.createElement('div');
    this.pastePanel.className = 'spec-panel spec-paste-panel';
    this.pastePanel.style.display = 'none';

    this.textarea = document.createElement('textarea');
    this.textarea.className = 'spec-textarea';
    this.textarea.placeholder = 'GitHub Issue の要件、PR の説明文、受け入れ基準、または設計書のテキストをここに貼り付けてください...';
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
    clearPasteBtn.title = '入力をクリア';
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

  public setFiles(files: string[]): void {
    this.files = [...files];
    this.renderFileOptions();
    if (this.selectedFile && !this.files.includes(this.selectedFile)) {
      this.selectedFile = '';
      this.selectedSection = '';
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

  public getParam(): SpecParam | null {
    if (!this.enabled) {
      return null;
    }
    if (this.mode === 'file') {
      if (!this.selectedFile) {
        return null;
      }
      return {
        source_type: 'file',
        file_path: this.selectedFile,
        section: this.selectedSection || null,
      };
    } else {
      const trimmed = this.pasteText.trim();
      if (!trimmed) {
        return null;
      }
      return {
        source_type: 'paste',
        paste_text: trimmed,
      };
    }
  }

  public isValid(): boolean {
    if (!this.enabled) {
      return true;
    }
    if (this.mode === 'file') {
      return Boolean(this.selectedFile);
    } else {
      const chars = this.pasteText.trim().length;
      return chars > 0 && chars <= MAX_SPEC_CHARS;
    }
  }

  public async selectFile(filePath: string): Promise<void> {
    this.selectedFile = filePath.trim();
    this.filePathInput.value = this.selectedFile;
    this.fileSelect.value = this.selectedFile;
    await this.handleFileChange();
    this.notifyChange();
  }

  public clearFile(): void {
    this.selectedFile = '';
    this.selectedSection = '';
    this.filePathInput.value = '';
    this.fileSelect.value = '';
    this.sectionSelect.innerHTML = '';
    this.sectionSelect.disabled = true;
    this.sectionSelect.style.display = 'none';
    this.notifyChange();
  }

  public setDisabled(disabled: boolean): void {
    this.toggleCheckbox.disabled = disabled;
    this.tabFileBtn.disabled = disabled;
    this.tabPasteBtn.disabled = disabled;
    this.pickFileBtn.disabled = disabled;
    this.filePathInput.disabled = disabled;
    this.fileSelect.disabled = disabled;
    this.sectionSelect.disabled = disabled || !this.selectedFile;
    this.textarea.disabled = disabled;
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
    this.cardTitle.textContent = '📘 参照する設計書・仕様書の指定';
    this.tabFileBtn.textContent = this.messages.specView.tabFile;
    this.tabPasteBtn.textContent = this.messages.specView.tabPaste;
    this.renderFileOptions();
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

    this.fileSelect.innerHTML = '';
    const placeholderOpt = document.createElement('option');
    placeholderOpt.value = '';
    placeholderOpt.textContent = this.messages.specView.fileSelectPlaceholder;
    this.fileSelect.appendChild(placeholderOpt);
    for (const f of this.files) {
      const opt = document.createElement('option');
      opt.value = f;
      opt.textContent = f;
      this.fileSelect.appendChild(opt);
    }
    this.fileSelect.value = this.selectedFile;
  }

  private async handleFileChange(): Promise<void> {
    this.sectionSelect.innerHTML = '';
    this.selectedSection = '';

    if (!this.selectedFile) {
      this.sectionSelect.disabled = true;
      this.sectionSelect.style.display = 'none';
      return;
    }

    const wholeFileOpt = document.createElement('option');
    wholeFileOpt.value = '';
    wholeFileOpt.textContent = this.messages.specView.allSections;
    this.sectionSelect.appendChild(wholeFileOpt);

    if (this.onFileSelect) {
      try {
        const sections = await this.onFileSelect(this.selectedFile);
        if (sections && sections.length > 0) {
          for (const sec of sections) {
            const opt = document.createElement('option');
            opt.value = sec.title;
            const indent = '　'.repeat(Math.max(0, sec.level - 1));
            opt.textContent = `${indent}${sec.title}`;
            this.sectionSelect.appendChild(opt);
          }
          this.sectionSelect.disabled = false;
          this.sectionSelect.style.display = '';
        } else {
          this.sectionSelect.disabled = true;
          this.sectionSelect.style.display = 'none';
        }
      } catch (e) {
        console.error('Failed to load sections for spec file', e);
        this.sectionSelect.disabled = true;
        this.sectionSelect.style.display = 'none';
      }
    } else {
      this.sectionSelect.disabled = true;
      this.sectionSelect.style.display = 'none';
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
      this.onChange(this.getParam(), this.isValid());
    }
  }
}
