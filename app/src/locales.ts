// 多言語の辞書定義。新しい言語（de / fr / es / pt など）を足すときは、
// 1. `SUPPORTED_LANGS` に言語コードと表示名を足す
// 2. `MESSAGES` に同じ型のエントリを足す（TypeScript が不足キーをすべて検出する）
// これだけで UI コンポーネント側のコードには一切手を触れずに言語を増やせる。

import type { SettingMeta, TriageState } from './types';

export const SUPPORTED_LANGS = [
  { code: 'ja', nativeName: '日本語', englishName: 'Japanese', bcp47: 'ja-JP' },
  { code: 'en', nativeName: 'English', englishName: 'English', bcp47: 'en-US' },
] as const;

export type Lang = (typeof SUPPORTED_LANGS)[number]['code'];

export interface SettingMetaText {
  label: string;
  description: string;
  why: string;
  when: string;
  unit?: string;
}

export interface SettingsTabDef {
  id: string;
  label: string;
  desc: string;
  groups: string[];
}

export interface Messages {
  bcp47: string;
  listSeparator: string;
  noneParen: string;
  autoLanguageLabel: string;
  autoLanguageOptionLabel: string;
  languageNames: Record<Lang, string>;

  // triageview.ts
  triageChip: {
    doneText: string;
    doneTitle: string;
    openText: string;
    openTitle: string;
  };

  // specview.ts & detailview.ts
  specView: {
    toggleLabel: string;
    tabFile: string;
    tabPaste: string;
    cardTitle: string;
    pickFile: string;
    pickFileTitle: string;
    filePathPlaceholder: string;
    clearSelectionTitle: string;
    pasteAreaPlaceholder: string;
    clearInputTitle: string;
    fileSelectPlaceholder: string;
    noFilesFound: string;
    allSections: string;
    sectionSelectPlaceholder: string;
    pastePlaceholder: string;
    charCount: (chars: number, tokens: number) => string;
    charCountOver: (chars: number, max: number) => string;
    clearButton: string;
    specBadge: string;
    specQuoteLabel: string;
    specUnverifiedBadge: string;
    specVerifiedBadge: string;
    addedCount: (n: number, max: number) => string;
    removeFile: string;
    maxFilesReached: (max: number) => string;
  };

  // recipeinfo.ts
  recipeSummaryMap: Record<string, string>;
  recipeGroupLlm: string;
  recipeGroupTool: string;
  recipeBadgeRepo: string;

  // settingsmodel.ts & settingsui.ts
  formatRange: (min: string, max: string, unit: string) => string;
  groupNames: Record<string, string>;
  settingMeta: Record<string, SettingMetaText>;
  settingsTabs: SettingsTabDef[];
  otherTab: SettingsTabDef;
  settingsUi: {
    title: string;
    intro1: string;
    introFile: (path: string, exists: boolean) => string;
    repoInvalid: (path: string, err: string) => string;
    repoOverrides: (path: string, keysText: string) => string;
    fileInvalid: (err: string) => string;
    save: string;
    saving: string;
    undo: string;
    resetAll: string;
    close: string;
    saveAndClose: string;
    discardAndClose: string;
    keepEditing: string;
    validationErrorsCount: (n: number) => string;
    unsavedChangesCount: (n: number) => string;
    tallyText: (changed: number, dirty: number, errors: number) => string;
    confirmUnsaved: (n: number) => string;
    cannotSaveWithErrors: string;
    onePerLine: string;
    customizedBadge: string;
    resetToDefault: string;
    overriddenByRepo: string;
    canOverrideByRepo: string;
    personalSettingOnly: string;
    defaultLabel: (val: string) => string;
    rangeLabel: (range: string) => string;
    whySummary: string;
    whyPrefix: string;
    whenPrefix: string;
    savedToast: string;
    savedWithErrors: (errs: string[]) => string;
  };

  // agentconsent.ts
  agentConsent: {
    title: string;
    desc: string;
    checking: string;
    consentedStrong: string;
    consentedBody: string;
    revokeBtn: string;
    revokedNote: string;
    notConsentedStrong: string;
    notConsentedBody: string;
  };

  // hookview.ts
  hookUi: {
    tabLabel: string;
    title: string;
    desc: string;
    noRepo: string;
    loading: string;
    stateOn: string;
    stateOff: string;
    enable: string;
    disable: string;
    switchLabel: string;
    busy: string;
    recipesLabel: string;
    recipesNote: string;
    saveRecipes: string;
    noRecipes: string;
    needsSpecBadge: string;
    specLabel: string;
    specHint: string;
    specPlaceholder: string;
    groupCode: string;
    groupTf: string;
    groupDoc: string;
    groupOther: string;
    groupCodeDesc: string;
    groupTfDesc: string;
    groupDocDesc: string;
    groupOtherDesc: string;
    selectAll: string;
    clearAll: string;
    groupCount: (picked: number, total: number) => string;
    installedYes: string;
    installedNo: string;
    cliMissing: (brewCmd: string, npmCmd: string) => string;
    targetMissing: string;
    cliPath: (path: string) => string;
    cliWillUse: (path: string) => string;
    cliDiffers: (path: string) => string;
    blocked: (why: string) => string;
    costNote: string;
    keyNote: string;
    lastRunTitle: string;
    lastRunNone: string;
    refresh: string;
    refreshFailed: (err: string) => string;
    logUnreadable: (n: number) => string;
    runCommit: (commit: string, subject: string) => string;
    runNoCommit: string;
    runKind: string;
    runState: (state: string) => string;
    runSummary: (findings: number | null, secs: string | null) => string;
    stepOk: (findings: number) => string;
    stepFailed: string;
    stepRunning: string;
    secs: (ms: number) => string;
    failed: (err: string) => string;
  };

  // repoview.ts
  repoView: {
    emptyTitle: string;
    emptyHint: string;
    appIconTitle: (title: string) => string;
    renameTitle: string;
    setNameTitle: string;
    unlinkTitle: string;
    openCountTitle: (n: number) => string;
    branchTitle: string;
    colRepo: string;
    colBranch: string;
    colLast: string;
    colHook: string;
    colOpen: string;
    lastReview: (when: string) => string;
    noReview: string;
    hookOn: string;
    hookOff: string;
    hookCheck: string;
    hookOnTitle: string;
    hookOffTitle: string;
    hookCheckTitle: string;
    heading: string;
    addTitle: string;
    addBtn: string;
  };

  // recipebar.ts
  recipeBar: {
    groupDocs: string;
    runningLabel: (progress: string) => string;
    selectAngles: string;
    runSingle: (title: string) => string;
    runFallback: string;
    runMultiple: (count: number) => string;
    /** 実行できない観点を外して走らせるときの、ボタンの文言 */
    runSkipping: (count: number, what: string) => string;
    skipWhatSpec: string;
    skipWhatLlm: string;
    skipWhatMixed: string;
    /** 外した観点のカードに付ける印 */
    skipBadge: string;
    /** 外した観点の注記（理由つき） */
    skipNotice: (titles: string[], reason: string) => string;
    /** 「前回のレビューの続き」を選んでいて、前回の実行に含まれない観点があるとき */
    lastReviewNote: (titles: string[]) => string;
    expandNewReview: string;
    showAllRecipes: (hidden: number) => string;
    showFitOnly: string;
    selectAll: string;
    clearAll: string;
    estTokens: (tokens: string) => string;
    estTime: (time: string) => string;
    cancel: string;
  };


  // baseinfo.ts
  baseInfo: {
    uncommitted: string;
    entireRepo: string;
    fromCommit: (hash7: string, subject: string) => string;
    scopeSuffix: string;
    onlyUncommitted: string;
    lastCommitAndUncommitted: string;
    allChangesFromForward: (name: string, scope: string) => string;
    allChangesAfterBranch: (branch: string, scope: string) => string;
    allChangesFromBase: (label: string, scope: string) => string;
    groupRange: string;
    groupRecent: string;
    groupOther: string;
    latestCommitTitle: string;
    entireRepoHint: string;
    fromCustomTitle: (spec: string) => string;
    customDesc: string;
    customItemTitle: string;
    customItemDesc: string;
    translateBaseHint: (desc: string) => string;
    normalizeBaseTitle: (value: string, label: string) => { title: string; desc?: string } | null;
    lastReviewTitle: (when: string) => string;
    lastReviewDesc: (short: string, ran: string) => string;
    allChangesSinceLastReview: (when: string, scope: string) => string;
  };

  // historyview.ts
  builtinRecipeTitles: Record<string, string>;
  historyView: {
    formatDay: (d: Date) => string;
    countUnit: (label: string, n: number) => string;
    countsTitle: string;
    countOpen: string;
    countDone: string;
    countWait: string;
    countDismiss: string;
    selectForReportAria: string;
    failedBadge: string;
    detailTitle: string;
    detailBtn: string;
    findingsCount: (n: number) => string;
    showMore: (n: number) => string;
    groupCount: (n: number) => string;
    expandHistory: string;
    collapseHistory: string;
    heading: string;
    deselectAll: string;
    selectAll: string;
    selectedCount: (n: number) => string;
    exportReportBtn: string;
    deleteBtn: string;
    deleteConfirm: (n: number) => string;
    deleteOk: string;
  };

  // usageview.ts
  usageView: {
    periods: { today: string; week: string; all: string };
    colReviews: string;
    colInput: string;
    colOutput: string;
    failedSuffix: (n: number) => string;
    unreportedTitle: string;
    unreportedSuffix: (n: number) => string;
    noRecords: string;
    heading: string;
    subtitle: string;
    closeBtn: string;
    calculating: string;
    cardInput: string;
    cardOutput: string;
    cardCacheRead: string;
    cardCacheWrite: string;
    totalReviews: (n: string) => string;
    totalFailedExcluded: (n: number) => string;
    totalUnreportedExcluded: (n: number) => string;
    unreadableError: (n: number) => string;
    reportedCost: (usd: string, count: number) => string;
    byAngle: string;
    byRepo: string;
    byModel: string;
    footnote: string;
  };

  // reportview.ts
  reportView: {
    periods: { today: string; week: string; month: string; all: string; custom: string };
    invalidStartDate: string;
    invalidEndDate: string;
    startAfterEnd: string;
    fileNameSelected: (count: number, date: string, ext: string) => string;
    fileNameAll: string;
    fileNameRepo: (safeRepo: string, date: string, ext: string) => string;
    heading: string;
    subtitle: string;
    pickedTargeting: (count: number) => string;
    switchToDateRange: string;
    closeBtn: string;
    dateRangeSep: string;
    selectedRepoBtn: string;
    allReposBtn: string;
    includeEvidence: string;
    includeTokens: string;
    includeLog: string;
    includeLogHint: string;
    includePrompt: string;
    includePromptHint: string;
    secretNotice: string;
    loading: string;
    summaryLine: (reviews: string, findings: string, fix: number, wait: number, dismiss: number, fresh: number) => string;
    savingBtn: string;
    saveMdBtn: string;
    saveXlsxBtn: string;
    printTitle: string;
    openingPrintBtn: string;
    exportPdfBtn: string;
    generatingPreview: string;
  };

  // main.ts
  mainUi: {
    repoDataSummary: (d: { reviews: number; findings: number; triaged: number; memos: number }) => string;
    repoDeleteConfirmTitle: (title: string) => string;
    repoDeleteConfirmBody: (summary: string) => string;
    repoDeleteCannotUndo: string;
    repoDeleteExportHiddenHint: string;
    repoDeleteExportVisibleHint: string;
    openReportBtn: string;
    deleteAndFreeSlotBtn: string;
    cancelBtn: string;
    deleteDataTitle: string;
    deleteDataBtn: string;
    displayNameAria: (title: string) => string;
    inUseChip: string;
    subdirNote: (root: string) => string;
    moveUpTitle: string;
    moveDownTitle: string;
    unlinkWithHistoryTitle: string;
    unlinkBtn: string;
    restoreBtn: string;
    removeEmptyTitle: string;
    removeEmptyBtn: string;
    registeredReposHeading: string;
    registeredReposIntro1: (limit: number | null) => string;
    registeredReposIntro2: string;
    slotsLabel: string;
    slotsUnlimited: (used: number) => string;
    slotsOverLimit: string;
    slotsFull: string;
    addRepoBtn: string;
    noReposRegistered: string;
    unlinkedHoldingSlots: (count: number) => string;
    savedReportToast: (path: string) => string;
    rangePrefix: string;
    baseInputPlaceholder: string;
    baseFormHint: string;
    baseFormSetBtn: string;
    baseFormCancelBtn: string;
    pageTabsLabel: string;
    pageTabRepos: string;
    pageTabHistory: string;
    hookRunsHeading: string;
    branchTitle: string;
    historySearchPlaceholder: string;
    historySearchLabel: string;
    historySearchCount: (shown: number, total: number) => string;
    historySearchClear: string;
    historySearchNoMatch: string;
    runRecordSummary: string;
    pageTabNewReview: string;
    usageTitle: string;
    usageBtn: string;
    reportTitle: string;
    reportBtn: string;
    settingsTitle: string;
    settingsBtn: string;
    llmHeading: string;
    llmSubtitle: string;
    byokKeyPlaceholder: string;
    byokLoading: string;
    byokUnsupported: string;
    byokVerifyErr: (err: string) => string;
    byokConfigured: (last4: string) => string;
    byokNotConfigured: string;
    byokHeading: string;
    byokNote: string;
    byokKeyLabel: string;
    byokReplaceBtn: string;
    byokSaveBtn: string;
    byokConfirmDeleteBtn: string;
    byokDeleteBtn: string;
    byokModelLabel: string;
    byokMaxTokensLabel: string;
    byokInvalidConfig: (err: string) => string;
    recipeInfoToolTitle: (title: string) => string;
    recipeInfoRepoSource: string;
    recipeInfoSummary: (count: number) => string;
    sendBoxTitleTool: string;
    sendBoxTitleLlm: string;
    sendBoxCannotRead: string;
    sendBoxSummaryTool: (title: string, count: number) => string;
    sendBoxSummaryLlm: (title: string, count: number, tokens: string, excluded: number) => string;
    sendBoxRangeLabel: string;
    sendBoxDiffSep: string;
    sendBoxBytesSuffix: (bytes: string) => string;
    sendBoxExcludedHeading: (count: number) => string;
    sendBoxNoDiff: string;
    sendBoxSelectRepoSummary: (title: string) => string;
    sendBoxSelectRepoBody: string;
    rightPanelAllRepos: string;
    rightPanelEmptyRepo: string;
    rightPanelEmptyAll: string;
    triageLabels: [TriageState, string][];
    triageResetTitle: string;
    chipEvidenceMismatch: string;
    chipEvidenceMismatchTitle: (groundingNote: string) => string;
    chipSpecVerified: string;
    chipSpecMismatch: string;
    specQuoteVerified: string;
    specQuoteMismatch: string;
    specQuoteLabel: string;
    specQuoteSource: (name: string) => string;
    chipAgentDid: string;
    chipAgentSkipped: string;
    chipAgentTitle: string;
    chipMemo: string;
    chipCarried: string;
    chipCarriedTitle: (stateLabel: string, stateRaw: TriageState) => string;
    chipNew: string;
    chipNewTitle: string;
    chipPersist: (no: number) => string;
    chipPersistTitle: string;
    /** 結果の一覧の上の、仕分けを始める前のヒント */
    handoffHint: string;
    /** 結果の先頭の「前回 → 今回」の帯・流れの行・前回の指摘の行方・「前回の続きから」の文言 */
    /** 実行の計画（`runplan.ts`）が、実行できない理由として出す文言 */
    runPlan: {
      noRepo: string;
      noLlm: string;
      noSpec: string;
      badSpec: string;
      noDiffUncommitted: string;
      noDiffRange: string;
    };
    continuity: {
      stateLabel: Record<TriageState, string>;
      carryNote: (stateLabel: string, exact: boolean) => string;
      prev: string;
      prevTitle: (when: string) => string;
      prevRun: (name: string, findings: number) => string;
      thisRun: (name: string, findings: number) => string;
      persist: string;
      gone: string;
      fresh: string;
      other: string;
      chipOnTitle: string;
      chipOffTitle: string;
      note: string;
      flowLabel: (title: string) => string;
      flowItem: (seq: number, failed: boolean, findings: number) => string;
      flowNowTitle: string;
      flowOpenTitle: (when: string) => string;
      panelTitle: (name: string) => string;
      panelTitleDefault: string;
      noTitle: string;
      fateBefore: (stateLabel: string, file: string) => string;
        fate: Record<'persists' | 'gone' | 'not_reraised' | 'file_missing' | 'unknown', { badge: string; text: string }>;
      continueBtn: (name: string, findings: number) => string;
      continueTitle: (when: string) => string;
      continueFailed: (reason: string) => string;
    };
    resultEmpty: string;
    tabFindings: (count: number) => string;
    tabDiff: string;
    tabRecord: string;
    reportThisReviewTitle: string;
    reportThisReviewBtn: string;
    backToHistory: string;
    closeResult: string;
    runLogDetail: {
      groupRun: string;
      groupInput: string;
      recipe: string;
      provider: string;
      duration: string;
      tokens: string;
      tokensValue: (input: number | null, output: number | null) => string;
      cost: string;
      failure: string;
      branch: string;
      base: string;
      baseWorking: string;
      baseCommit: (spec: string, subject: string) => string;
      diff: string;
      diffValue: (files: number, bytes: number) => string;
      specs: string;
      none: string;
      related: string;
      relatedValue: (snippets: number) => string;
      prompt: string;
      promptValue: (chars: number) => string;
      loading: string;
      loadFailed: string;
      notFound: string;
      diffSummary: string;
      diffMissing: string;
    };
    viewDetailBtn: string;
    closeDetailBtn: string;
    recordSubDetail: string;
    recordSubSent: string;
    recordSubResponse: string;
    execFailed: (err: string) => string;
    allowAndRerunBtn: string;
    parseFailed: (err: string) => string;
    noFindings: string;
    expandAllBtn: string;
    collapseAllBtn: string;
    fixAllSevBtn: (sev: string, n: number) => string;
    dismissAllSevBtn: (sev: string, n: number) => string;
    quoteVerified: string;
    quoteNotVerified: string;
    noQuote: string;
    memoPlaceholder: string;
    markDoneBtn: string;
    doneRevertBtn: string;
    triageNoTimestamp: string;
    triageCarrySource: string;
    triageUserSource: string;
    triageLastStamp: (whenStr: string, who: string, head7: string | null) => string;
    idLabel: string;
    copyBtn: string;
    copiedBtn: string;
    copyFailedBtn: string;
    sendToCliBtn: string;
    writingCliBtn: string;
    sendToCliTitle: string;
    sendAllToCliBtn: (count: number) => string;
    viewAllHandoffSummary: (count: number) => string;
    copyHandoffTextBtn: string;
    /** 「まとめて渡す内容」の横の、MCP で直接対応できるという案内（1 行） */
    mcpHint: string;
    fetchAgentBtn: string;
    fetchingAgentBtn: string;
    fetchAgentTitle: string;
    /** 申告の状態。全部に申告がある */
    agentStateAll: (reported: number, total: number) => string;
    /** 申告の状態。一部だけ */
    agentStatePartial: (reported: number, total: number, missing: number) => string;
    /** 申告の状態。1 件も無い */
    agentStateNone: (missing: number) => string;
    /** 申告の状態に添える。「対応しなかった」と申告された指摘は、あなたの判断が、まだ残っている */
    agentStateDecide: (skipped: number) => string;
  };
}

export const MESSAGES: Record<Lang, Messages> = {
  ja: {
    bcp47: 'ja-JP',
    listSeparator: '、',
    noneParen: '（なし）',
    autoLanguageLabel: '自動（システムの言語）',
    autoLanguageOptionLabel: '自動（システムの設定に従う） / Auto (System)',
    languageNames: {
      ja: '日本語',
      en: 'English',
    },

    triageChip: {
      doneText: '対応済み',
      doneTitle: '直し済みです（履歴の「済み」に数えます）',
      openText: '未対応',
      openTitle: 'これから直す指摘です（まだ直していません）。直したら、開いて「対応済みにする」を押します',
    },

    specView: {
      toggleLabel: '設計書・要件を参照する',
      tabFile: '📄 ファイルから選択',
      tabPaste: '📝 直接貼り付け',
      cardTitle: '📘 参照する設計書・仕様書の指定（複数可）',
      pickFile: '📄 ファイルを選択...',
      pickFileTitle: 'macOSのファイル選択ダイアログを開きます',
      filePathPlaceholder: 'ファイルパス（例: docs/spec.md）を入力して Enter、または「ファイルを選択...」で追加',
      clearSelectionTitle: '選択を解除',
      pasteAreaPlaceholder: 'GitHub Issue の要件、PR の説明文、受け入れ基準、または設計書のテキストをここに貼り付けてください...',
      clearInputTitle: '入力をクリア',
      fileSelectPlaceholder: 'ファイルを選択（.md / .txt）',
      noFilesFound: '参照できる仕様書ファイル（.md / .txt）が見つかりません',
      allSections: '（ファイル全体を参照）',
      sectionSelectPlaceholder: '対象の節・見出しを選択',
      pastePlaceholder: 'GitHub Issue、Notionの要件、仕様メモなどを貼り付けてください...',
      charCount: (chars, tokens) => `${chars.toLocaleString()} 文字（推定 ${tokens.toLocaleString()} トークン）`,
      charCountOver: (chars, max) => `⚠ 上限超過: ${chars.toLocaleString()} / 最大 ${max.toLocaleString()} 文字（節を絞り込むか削ってください）`,
      clearButton: 'クリア',
      specBadge: '設計書参照',
      specQuoteLabel: '設計書の要求（原文引用）',
      specUnverifiedBadge: '根拠不一致（設計書に原文なし）',
      specVerifiedBadge: '設計書照合済',
      addedCount: (n, max) => `${n} / ${max} 件を指定中`,
      removeFile: '外す',
      maxFilesReached: (max) => `設計書は最大 ${max} 件までです`,
    },

    recipeSummaryMap: {
      bugs: '動かすと壊れる書き方（境界・null・競合など）を探す',
      errors: '握りつぶし・握り損ね・失敗の見落としを探す',
      security: '入力の検証・認証・秘密の扱いなどの穴を探す',
      performance: '無駄な繰り返し・重い処理・N+1 などを探す',
      tests: 'テストが足りない所・弱い検証を探す',
      consistency: '変えた所と、変えていない他の場所の食い違いを探す',
      spec: '設計書・仕様書と差分を見比べ、矛盾や要求への逆行を探す',
    },
    recipeGroupLlm: 'LLM でレビュー',
    recipeGroupTool: 'LLM を使わない',
    recipeBadgeRepo: 'このリポジトリ',

    formatRange: (min, max, unit) => `${min}〜${max}${unit}`,
    groupNames: {},
    settingMeta: {},
    settingsTabs: [
      { id: 'llm', label: 'LLM・API キー', desc: 'レビューに使う LLM と、自分の API キー（BYOK）。', groups: [] },
      { id: 'repos', label: 'リポジトリ', desc: '登録したリポジトリの、名前・並び順・登録の解除。', groups: [] },
      { id: 'diff', label: '送る内容', desc: 'LLM に送る差分と、差分に出ていない関連コードの上限。', groups: ['送る差分', '関連コード'] },
      { id: 'run', label: '実行', desc: 'LLM やツールの待ち時間と、実行の上限。', groups: ['実行'] },
      { id: 'triage', label: '仕分け', desc: '前回の仕分けの引き継ぎ。', groups: ['仕分けの引き継ぎ'] },
      { id: 'display', label: '画面', desc: '履歴の表示件数と、表示言語。', groups: ['画面'] },
      { id: 'link', label: '連携', desc: 'エージェントの登録と、会話の記録を読む許可。', groups: ['連携'] },
      { id: 'hook', label: 'Git フック', desc: 'コミットのあとに、自動でレビューを走らせる。', groups: [] },
    ],
    otherTab: { id: 'other', label: 'その他', desc: 'ほかのタブに入らない項目', groups: [] },
    settingsUi: {
      title: '設定',
      intro1: '初期値は、標準値・推奨値です。変えたい項目だけ変えてください。推奨と違う項目には印が付き、いつでも推奨値に戻せます。',
      introFile: (path, exists) => `設定ファイル: ${path}（${exists ? 'あります' : 'まだありません。保存すると作られます'}）。推奨と違う項目だけが書かれます。`,
      repoInvalid: (_path, err) => `このリポジトリの設定ファイルが正しくありません（レビューは、これを直すまで実行できません）: ${err}`,
      repoOverrides: (path, keysText) =>
        `このリポジトリには設定ファイル（${path}）があり、次の項目を上書きしています: ${keysText}。これらの項目は、ここで変えても、このリポジトリでは、リポジトリ側の値が使われます。`,
      fileInvalid: (err) =>
        `設定ファイルが正しくありません: ${err}。この画面は推奨値で開いています。保存すると、元のファイルを settings.json.bak に残してから作り直します。`,
      save: '保存',
      saving: '保存中…',
      undo: '変更を取り消す',
      resetAll: 'すべて推奨値に戻す',
      close: '閉じる',
      saveAndClose: '保存して閉じる',
      discardAndClose: '破棄して閉じる',
      keepEditing: '戻る',
      validationErrorsCount: (n) => `入力に誤りが ${n} 件あります`,
      unsavedChangesCount: (n) => `未保存の変更が ${n} 件あります`,
      tallyText: (changed, dirty, errors) =>
        `推奨と違う項目 ${changed} 件` +
        (dirty ? ` ・ 未保存の変更 ${dirty} 件` : '') +
        (errors ? ` ・ 入力に誤り ${errors} 件` : ''),
      confirmUnsaved: (n) => `未保存の変更が ${n} 件あります。保存して閉じますか？`,
      cannotSaveWithErrors: '入力に誤りがあるので、保存できません。直すか、「破棄して閉じる」を選んでください',
      onePerLine: '1 行に 1 つ',
      customizedBadge: '推奨と違う',
      resetToDefault: '推奨に戻す',
      overriddenByRepo: 'このリポジトリが上書き中',
      canOverrideByRepo: 'リポジトリ側でも変えられる',
      personalSettingOnly: 'あなた個人の設定だけ',
      defaultLabel: (val) => `推奨: ${val}`,
      rangeLabel: (range) => `（範囲: ${range}）`,
      whySummary: 'なぜこの値が推奨か・どんなときに変えるか',
      whyPrefix: 'なぜ推奨か: ',
      whenPrefix: 'いつ変えるか: ',
      savedToast: '保存しました。次のレビューから有効です。',
      savedWithErrors: (errs) => `設定は保存しましたが、一部の反映に失敗しました: ${errs.join(' ／ ')}`,
    },

    agentConsent: {
      title: '会話の記録の読み取り',
      desc: '「対応内容を取り込む」は、Claude Code・agy・GitHub Copilot CLI の会話の記録から、エージェントが書いた「最後の報告」の行だけを探します。会話の本文は、保存しません。',
      checking: '許可の状態を読み取っています…',
      consentedStrong: '許可済み',
      consentedBody: '。「対応内容を取り込む」を押すと、確認なしで、会話の記録を読みます。',
      revokeBtn: '許可を取り消す',
      revokedNote: '取り消しても、取り込み済みの報告は、残ります。次に「対応内容を取り込む」を押したとき、もう一度、読んでよいかを聞きます。',
      notConsentedStrong: '未許可',
      notConsentedBody: '（まだ許可していないか、取り消しました）。「対応内容を取り込む」を初めて押したときに、読んでよいかを聞きます。',
    },

    hookUi: {
      tabLabel: 'Git フック',
      title: 'コミットのあとの自動レビュー',
      desc: 'このリポジトリでコミットするたびに、直前のコミットだけを自動でレビューします。結果は、通常のレビュー履歴に入ります。コミットは止めません（止める機能は、まだありません）。',
      noRepo: '左のリポジトリを選ぶと、設定できます。',
      loading: '状態を読み取っています…',
      stateOn: 'ON（コミットのたびにレビューします）',
      stateOff: 'OFF',
      enable: 'ON にする',
      disable: 'OFF にする',
      switchLabel: 'コミットのあとの自動レビュー',
      busy: '処理しています…',
      recipesLabel: 'レビュー観点',
      recipesNote: '「設計書が要る」観点を選ぶときは、下で設計書を指定してください。',
      saveRecipes: '保存する',
      noRecipes: '選べる観点がありません。',
      needsSpecBadge: '設計書が要る',
      specLabel: '設計書（設計書が要る観点に渡します。1 行に 1 つ、最大 5 件）',
      specHint: 'このリポジトリの中のファイルを、リポジトリからの相対パスで指定します（.md / .txt。章の指定はできません）。リポジトリの外のファイルは、指定できません。',
      specPlaceholder: '例:\ndocs/requirements.md\ndocs/design.md',
      groupCode: 'コード',
      groupTf: 'Terraform（tfstate）',
      groupDoc: '文書',
      groupOther: 'このリポジトリ・自分で足した観点',
      groupCodeDesc: 'コードの変更を見る観点。設計書との照合（spec）も、ここです。',
      groupTfDesc: 'Terraform の変更（.tf など）を見る観点。コードの差分には、要りません。',
      groupDocDesc: '文書（README・設計書・構想書など）の変更を見る観点。',
      groupOtherDesc: 'このリポジトリの .mrcodereview/recipes に置いた観点や、自分で足した観点。',
      selectAll: 'すべて選ぶ',
      clearAll: 'すべて外す',
      groupCount: (picked, total) => `${picked} / ${total} 選択中`,
      installedYes: 'フックは、このリポジトリに入っています。',
      installedNo: 'フックは、このリポジトリに入っていません。',
      cliMissing: (brewCmd, npmCmd) =>
        `mrcr（CLI）が見つかりません。フックは CLI を呼び出すので、先に入れてください（例: ${brewCmd}、または ${npmCmd}）。npx で起動したものは、実行ファイルが一時のキャッシュにあり、あとで消えるので使えません。`,
      targetMissing: 'フックが呼ぶ mrcr が見つかりません（更新や削除で消えた可能性があります）。mrcr を入れ直してから、「OFF にする」→「ON にする」で、入れ直してください。',
      cliPath: (path) => `フックが呼び出す mrcr: ${path}`,
      cliWillUse: (path) => `ON にすると、フックが呼び出す mrcr: ${path}`,
      cliDiffers: (path) => `いま見つかる mrcr は ${path} です。OFF にしてから ON にし直すと、そちらに切り替わります。`,
      blocked: (why) => `このリポジトリには、自動では入れられません: ${why}`,
      costNote: 'コミットのたびに、LLM の API を使います（従量課金）。',
      keyNote:
        'フックから呼ばれる mrcr は、環境変数 ANTHROPIC_API_KEY を使います。ターミナルから git を使うなら、そのまま引き継がれます。見えないときは、失敗として下に出ます（コミットは止まりません）。',
      lastRunTitle: '実行ログ（新しい順）',
      lastRunNone: 'まだ実行されていません。',
      refresh: '更新',
      refreshFailed: (err) => `最新の状態を読み取れませんでした: ${err}`,
      logUnreadable: (n) => `実行ログに、読めない行が ${n} 行あります（ほかのリポジトリの分を含みます）。`,
      runCommit: (commit, subject) => `コミット ${commit}「${subject}」`,
      runNoCommit: '（コミットの記録なし）',
      runKind: 'フック',
      runState: (state) => (state === 'running' ? '実行中' : state === 'done' ? '完了' : state === 'failed' ? '失敗' : '中断'),
      runSummary: (findings, secs) => [findings === null ? '' : `指摘 ${findings} 件`, secs ?? ''].filter(Boolean).join('・'),
      stepOk: (findings) => `指摘 ${findings} 件`,
      stepFailed: '失敗',
      stepRunning: '実行中…',
      secs: (ms) => `${(ms / 1000).toFixed(1)} 秒`,
      failed: (err) => `操作に失敗しました: ${err}`,
    },

    repoView: {
      emptyTitle: '登録されたリポジトリがありません。',
      emptyHint: '「+ 追加」からリポジトリを登録してください。',
      appIconTitle: (title) => `${title} のアプリアイコン`,
      renameTitle: '表示名を変更',
      setNameTitle: '表示名をつける',
      unlinkTitle: '登録を解除',
      openCountTitle: (n) => `未対応の指摘 ${n} 件（未仕分けと、対応中。待ち・無視・済みは含みません）`,
      branchTitle: 'いま、チェックアウトしているブランチ',
      colRepo: 'リポジトリ',
      colBranch: 'ブランチ',
      colLast: '最後のレビュー',
      colHook: 'Git フック',
      colOpen: '未対応',
      lastReview: (when) => `最後のレビュー ${when}`,
      noReview: 'レビューはまだありません',
      hookOn: 'フック ON',
      hookOff: 'フック OFF',
      hookCheck: 'フック 要確認',
      hookOnTitle: 'コミットのあとに、自動でレビューします（Git フック）',
      hookOffTitle: 'Git フックは、使っていません（設定から入れられます）',
      hookCheckTitle: '設定は ON ですが、フックが入っていません。設定の「Git フック」で、入れ直してください',
      heading: 'リポジトリ',
      addTitle: 'リポジトリを追加',
      addBtn: '追加',
    },

    recipeBar: {
      groupDocs: '文書',
      runningLabel: (progress) => `実行中…${progress}`,
      selectAngles: '観点を選択してください',
      runSingle: (title) => `「${title}」を実行`,
      runFallback: '実行',
      runMultiple: (count) => `選択した ${count} 件を実行`,
      runSkipping: (count, what) => `${count} 件を実行（${what}は除く）`,
      skipWhatSpec: '設計書の観点',
      skipWhatLlm: 'LLM が要る観点',
      skipWhatMixed: '実行できない観点',
      skipBadge: '今回は実行しない',
      skipNotice: (titles, reason) => `${titles.map((t) => `「${t}」`).join('')}は、今回は実行しません。${reason}`,
      lastReviewNote: (titles) => `${titles.map((t) => `「${t}」`).join('')}は、前回の実行に含まれていないので、前回より前の変更は見ません。`,
      expandNewReview: '▸ 新しいレビュー',
      showAllRecipes: (hidden) => `すべての観点を見る（他に ${hidden} 件）`,
      showFitOnly: 'この差分に向く観点だけ',
      selectAll: 'すべて選択',
      clearAll: '全解除',
      estTokens: (tokens) => `入力目安 約 ${tokens} トークン`,
      estTime: (time) => `目安時間 ${time}`,
      cancel: '中止',
    },


    baseInfo: {
      uncommitted: '未コミットの変更',
      entireRepo: '最初から（リポジトリ全体）',
      fromCommit: (hash7, subject) => `${hash7} ${subject} から`,
      scopeSuffix: '（コミット済み・未コミット・新規ファイル）',
      onlyUncommitted: 'コミットしていない変更だけ',
      lastCommitAndUncommitted: '最後のコミットと、コミットしていない変更',
      allChangesFromForward: (name, scope) => `${name} 以降すべて${scope}`,
      allChangesAfterBranch: (branch, scope) => `${branch} から分かれたあとの変更すべて${scope}`,
      allChangesFromBase: (label, scope) => `${label.replace(/ から$/, '')} から、いままでの変更すべて${scope}`,
      groupRange: '範囲を選ぶ',
      groupRecent: '最近のコミットから',
      groupOther: 'ほか',
      latestCommitTitle: '直近のコミット',
      entireRepoHint: '最初のコミットから、いまの状態まで。大きいリポジトリでは、上限で途中までになります',
      fromCustomTitle: (spec) => `${spec} から`,
      customDesc: '入力した場所',
      customItemTitle: '任意の場所…',
      customItemDesc: 'コミット・ブランチ・タグの名前を入力する',
      translateBaseHint: (desc) => desc,
      normalizeBaseTitle: () => null,
      lastReviewTitle: (when) => `前回のレビューの続き（${when}）`,
      lastReviewDesc: (short, ran) => `${short} から、いまの作業ツリーまで。前回の実行: ${ran}。前回のレビュー時に未コミットだった変更は、また出ます`,
      allChangesSinceLastReview: (when, scope) => `前回のレビュー（${when}）から、いままでの変更すべて${scope}`,
    },

    builtinRecipeTitles: {},
    historyView: {
      formatDay: (d) => `${d.getFullYear()}年${d.getMonth() + 1}月${d.getDate()}日`,
      countUnit: (label, n) => `${label} ${n} 件`,
      countsTitle: '未＝未仕分けと、対応中（まだ済んでいない）／済み＝対応済み、またはエージェントが「対応した」と申告',
      countOpen: '未',
      countDone: '済み',
      countWait: '待ち',
      countDismiss: '無視',
      selectForReportAria: 'レポートに出すレビューとして選ぶ',
      failedBadge: '失敗',
      detailTitle: 'このレビューの詳細（所要時間・トークン・見る範囲など）を開く',
      detailBtn: '詳細',
      findingsCount: (n) => `指摘 ${n} 件`,
      showMore: (n) => `さらに表示（残り ${n} 件）`,
      groupCount: (n) => `（${n} 件）`,
      expandHistory: '履歴を展開する',
      collapseHistory: '履歴を折り畳む',
      heading: '履歴',
      deselectAll: 'すべて外す',
      selectAll: 'すべて選ぶ',
      selectedCount: (n) => `${n} 件を選択中`,
      exportReportBtn: 'レポートに出す',
      deleteBtn: '選んだ履歴を削除',
      deleteConfirm: (n) => `選んだ履歴 ${n} 件を削除しますか？\n（仕分け・対応メモ・エージェントの申告も一緒に消えます。元に戻せません）`,
      deleteOk: '削除する',
    },

    usageView: {
      periods: { today: '今日', week: '7 日間', all: 'すべて' },
      colReviews: '件数',
      colInput: '入力',
      colOutput: '出力',
      failedSuffix: (n) => `（失敗 ${n}）`,
      unreportedTitle: '報告が無かったレビューがあり、その分は足していません',
      unreportedSuffix: (n) => ` ＋報告なし ${n} 件`,
      noRecords: 'この期間の記録はありません。',
      heading: 'トークン使用量',
      subtitle: '記録したレビューの、測った値だけを足しています。',
      closeBtn: '閉じる',
      calculating: '集計しています…',
      cardInput: '入力',
      cardOutput: '出力',
      cardCacheRead: 'キャッシュ読み取り',
      cardCacheWrite: 'キャッシュ書き込み',
      totalReviews: (n) => `レビュー ${n} 件`,
      totalFailedExcluded: (n) => `（失敗 ${n} 件は、トークンを足していません）`,
      totalUnreportedExcluded: (n) => ` ・ トークンの報告が無かったレビュー ${n} 件（0 として足さず、除いています）`,
      unreadableError: (n) =>
        `壊れていて読めなかった記録が ${n} 件あり、この集計に含まれていません（データベースが壊れている可能性があります）`,
      reportedCost: (usd, count) => `提供元が報告した金額（参考）: $${usd}（${count} 件分。サブスクでは請求されません）`,
      byAngle: '観点別',
      byRepo: 'リポジトリ別',
      byModel: 'LLM・モデル別',
      footnote:
        'LLM を使わない観点（過去のリンターなど）は LLM を呼ばないので、トークンは 0 です（測っての 0 で、報告なしではありません）。提供元が値を返さなかった項目は、推定で埋めていません。',
    },

    reportView: {
      periods: { today: '今日', week: '7 日間', month: '30 日間', all: 'すべて', custom: '期間を指定' },
      invalidStartDate: '開始の日付が正しくありません',
      invalidEndDate: '終了の日付が正しくありません',
      startAfterEnd: '開始の日付が、終了の日付より後になっています',
      fileNameSelected: (count, date, ext) => `レビュー記録_選択${count}件_${date}.${ext}`,
      fileNameAll: 'すべて',
      fileNameRepo: (safeRepo, date, ext) => `レビュー記録_${safeRepo}_${date}.${ext}`,
      heading: 'レポート',
      subtitle: 'いつ・何をして・どう対応したかを、成果物にできる形で出します。',
      pickedTargeting: (count) => `履歴から選んだレビュー ${count} 件を、対象にしています（期間とリポジトリの指定は、使いません）。`,
      switchToDateRange: '期間の指定に戻す',
      closeBtn: '閉じる',
      dateRangeSep: '〜',
      selectedRepoBtn: '選んでいるリポジトリ',
      allReposBtn: 'すべてのリポジトリ',
      includeEvidence: 'コードの引用を含める',
      includeTokens: 'トークンを含める',
      includeLog: '仕分けの経過を含める',
      includeLogHint: '（いつ何にしたか）',
      includePrompt: 'LLM に送った文章の全文を含める',
      includePromptHint: '（長くなります）',
      secretNotice:
        '成果物に入れるときは、コードの引用に、秘密の値（キー・パスワードなど）が混ざっていないか、出力する前に、プレビューで確認してください。',
      loading: '集めています…',
      summaryLine: (reviews, findings, fix, wait, dismiss, fresh) =>
        `レビュー ${reviews} 件・指摘 ${findings} 件（対応 ${fix}・待ち ${wait}・無視 ${dismiss}・未仕分け ${fresh}）`,
      savingBtn: '保存中…',
      saveMdBtn: 'Markdown で保存',
      saveXlsxBtn: 'Excel で保存',
      printTitle: '印刷の画面が開きます。「PDF」→「PDF として保存」を選んでください',
      openingPrintBtn: '印刷を開いています…',
      exportPdfBtn: 'PDF で出力',
      generatingPreview: 'プレビューを作ります…',
    },

    mainUi: {
      repoDataSummary: (d) =>
        d.reviews === 0 ? '履歴なし' : `履歴 ${d.reviews} 件・指摘 ${d.findings} 件・仕分け ${d.triaged} 件・メモ ${d.memos} 件`,
      repoDeleteConfirmTitle: (title) => `「${title}」の履歴などを、削除します。`,
      repoDeleteConfirmBody: (summary) => `消えるもの: ${summary}。リポジトリの中身は、消えません。`,
      repoDeleteCannotUndo: '元に戻せません。',
      repoDeleteExportHiddenHint: '書き出すなら、先に「復元」して、レポートを作ってください。',
      repoDeleteExportVisibleHint: '先に、レポート（Markdown・Excel・PDF）に書き出せます。',
      openReportBtn: 'レポートを開く',
      deleteAndFreeSlotBtn: '削除する',
      cancelBtn: 'やめる',
      deleteDataTitle: '履歴などを削除します',
      deleteDataBtn: 'データを削除…',
      displayNameAria: (title) => `${title} の表示名`,
      inUseChip: '使用中',
      subdirNote: (root) => `サブディレクトリを登録しています（リポジトリの根: ${root}）。履歴は、根のパスで結びつきます`,
      moveUpTitle: '上へ',
      moveDownTitle: '下へ',
      unlinkWithHistoryTitle: '一覧から外します（履歴は残ります。「復元」で戻せます）',
      unlinkBtn: '解除',
      restoreBtn: '復元',
      removeEmptyTitle: '履歴がないので、登録を消します',
      removeEmptyBtn: '登録を消す',
      registeredReposHeading: '登録したリポジトリ',
      registeredReposIntro1: (limit) =>
        limit === null
          ? '登録できるリポジトリの数に、上限はありません。「解除」は一覧から外すだけで、履歴は残ります（「復元」で戻せます。履歴が 1 件もないものは、登録ごと消えます）。履歴ごと消すには、「データを削除…」を使います。'
          : `${limit} リポジトリ分のデータまで使えます。「解除」は一覧から外すだけで、履歴があるあいだは、枠は戻りません（履歴が 1 件もないものは、登録ごと消えて、枠が戻ります）。枠を空けるには、「データを削除…」を使います。`,
      registeredReposIntro2:
        '表示名は、空にすると、フォルダ名になります。パスは変えられません（履歴が、パスに結びついているため）。リポジトリを移したときは、新しい場所を登録してください。',
      slotsLabel: '登録: ',
      slotsUnlimited: (used) => `${used} 件（上限なし）`,
      slotsOverLimit: '上限を超えています（新しい追加だけが止まります。いまの登録は、そのまま使えます）',
      slotsFull: '枠がいっぱいです',
      addRepoBtn: '+ リポジトリを追加…',
      noReposRegistered: '使用中のリポジトリが、ありません。',
      unlinkedHoldingSlots: (count) => `解除済み（履歴あり）${count} 件`,
      savedReportToast: (path) => `保存しました: ${path}`,
      rangePrefix: '見る範囲',
      baseInputPlaceholder: 'コミット・ブランチ・タグの名前（例: v1.2.0、feature/x、abc1234）',
      baseFormHint: 'ここから、いまの状態までの変更を見ます',
      baseFormSetBtn: '決める',
      baseFormCancelBtn: 'やめる',
      pageTabsLabel: 'ページ',
      pageTabRepos: 'リポジトリ',
      pageTabHistory: '履歴・ログ',
      hookRunsHeading: 'フックの実行',
      branchTitle: 'いま、チェックアウトしているブランチ',
      historySearchPlaceholder: '絞り込み（スペースで区切ると、すべてを満たすもの。例: 潜在バグ 昨日 未対応）',
      historySearchLabel: '履歴・ログを絞り込む',
      historySearchCount: (shown, total) => `${shown} 件 / 全 ${total} 件`,
      historySearchClear: '絞り込みを解除',
      historySearchNoMatch: '当てはまるものがありません。語を減らすか、解除してください。',
      runRecordSummary: '実行の記録（モデル・トークン・所要時間・入力・差分）',
      pageTabNewReview: '新規レビュー',
      usageTitle: 'トークン使用量',
      usageBtn: '使用量',
      reportTitle: 'レポート（成果物にできるレビューの記録）',
      reportBtn: 'レポート',
      settingsTitle: '設定（⌘,）',
      settingsBtn: '設定',
      llmHeading: 'LLM プロバイダ（BYOK / ローカル設定）',
      llmSubtitle: 'レビュー実行時に使用する LLM モデル・CLI プロバイダを指定します。',
      byokKeyPlaceholder: 'sk-ant-…（Anthropic API キー）',
      byokLoading: '読み込み中…',
      byokUnsupported: 'このマシンでは、キーを保管できません（いまは macOS のキーチェーンだけに対応）',
      byokVerifyErr: (err) => `キーを確かめられません: ${err}`,
      byokConfigured: (last4) => `設定済み（末尾 ${last4}）`,
      byokNotConfigured: '未設定',
      byokHeading: 'Anthropic API キー（BYOK）',
      byokNote:
        'キーは macOS のキーチェーンにだけ保存します（設定ファイル・履歴・ログには書きません）。保存したら、上の「LLM」で「Anthropic API」を選ぶと使えます。レビューの内容（差分・観点の文章）は、api.anthropic.com に送られます。',
      byokKeyLabel: 'キー: ',
      byokReplaceBtn: '置き換える',
      byokSaveBtn: '保存',
      byokConfirmDeleteBtn: '本当に消す',
      byokDeleteBtn: '消す',
      byokModelLabel: 'モデル名 ',
      byokMaxTokensLabel: '最大出力トークン ',
      byokInvalidConfig: (err) => `byok.json が正しくありません（いまは初期値を表示）: ${err}`,
      recipeInfoToolTitle: (title) => `${title}（LLM を使わない観点）`,
      recipeInfoRepoSource: '（設定）',
      recipeInfoSummary: (count) => `観点の中身を見る（${count} 件。LLM へ送る文章）`,
      sendBoxTitleTool: '対象の差分',
      sendBoxTitleLlm: '送る内容',
      sendBoxCannotRead: '読めません',
      sendBoxSummaryTool: (title, count) => `${title}: ${count} ファイル（LLM には送りません）`,
      sendBoxSummaryLlm: (title, count, tokens, excluded) =>
        `${title}: ${count} ファイル・約 ${tokens} トークン${excluded ? `・送らない ${excluded} 件` : ''}`,
      sendBoxRangeLabel: '見る範囲: ',
      sendBoxDiffSep: ' / 差分 ',
      sendBoxBytesSuffix: (bytes) => ` / ${bytes} バイト（トークンは推定値）`,
      sendBoxExcludedHeading: (count) => `送らない: ${count} 件`,
      sendBoxNoDiff: '差分がありません（HEAD からの変更が無い）。',
      sendBoxSelectRepoSummary: (title) => `${title}: リポジトリを選ぶと、ここに出ます`,
      sendBoxSelectRepoBody: 'リポジトリを選ぶと、どのファイルを送るか・送らないか・入力の概算がここに出ます。',
      rightPanelAllRepos: 'すべてのリポジトリ',
      rightPanelEmptyRepo: 'このリポジトリの履歴は、まだありません。',
      rightPanelEmptyAll: 'まだありません。',
      triageLabels: [
        ['fix', '対応'],
        ['wait', '待ち'],
        ['dismiss', '無視'],
      ],
      triageResetTitle: 'もう一度押すと未仕分けに戻る',
      chipEvidenceMismatch: '⚠ 根拠不一致',
      chipEvidenceMismatchTitle: (groundingNote) => `引用が、この差分に見つかりません。${groundingNote}`,
      chipSpecVerified: '設計書照合済',
      chipSpecMismatch: '根拠不一致（設計書に原文なし）',
      specQuoteVerified: '設計書の要求（原文引用・照合済）',
      specQuoteMismatch: '設計書の要求（原文引用・不一致）',
      specQuoteLabel: '設計書の要求（原文引用）',
      specQuoteSource: (name) => `出典: ${name}`,
      chipAgentDid: '対応した',
      chipAgentSkipped: '対応しなかった',
      chipAgentTitle: 'エージェントの申告・判断です。MCR が、コードを確かめたものではありません',
      chipMemo: '📝 メモ',
      chipCarried: '↺ 引き継ぎ',
      chipCarriedTitle: (stateLabel) => `前回までの仕分け「${stateLabel}」を引き継ぎました`,
      chipNew: '新規',
      chipNewTitle: '前回のレビューには無かった指摘です',
      chipPersist: (no) => `継続（前回 #${no}）`,
      chipPersistTitle: '前回の指摘と同じと判定しました。押すと、前回のレビューのその指摘へ移ります',
      handoffHint: '指摘を「対応」にすると、各指摘の横に「CLI に渡す」が出ます。',
      runPlan: {
        noRepo: 'リポジトリを選んでください（左サイドバー）',
        noLlm: 'LLM を選んでください（右上の設定から）',
        noSpec: '設計書を指定してください（ファイル選択または貼り付け）',
        badSpec: '設計書の内容を確認してください（文字数上限を超えているか無効です）',
        noDiffUncommitted: '送る差分がありません（未コミットの変更が無い）',
        noDiffRange: '送る差分がありません（この範囲に、変更が無い）',
      },
      continuity: {
        stateLabel: { new: '未仕分け', fix: '対応', wait: '待ち', dismiss: '無視' },
        carryNote: (stateLabel, exact) => `前回までの仕分け「${stateLabel}」を引き継ぎました` + (exact ? '' : '（引用がほぼ同じ）'),
        prev: '前回 ',
        prevTitle: (when) => `${when} に実行。押すと、そのレビューを開きます`,
        prevRun: (name, findings) => `${name}（指摘 ${findings} 件）`,
        thisRun: (name, findings) => `今回 ${name}（指摘 ${findings} 件）`,
        persist: '継続',
        gone: '解消',
        fresh: '新規',
        other: '再指摘なし・不明',
        chipOnTitle: 'もう一度押すと、絞り込みを外します',
        chipOffTitle: 'この種類だけに絞ります',
        note: '「解消」は、引用した行がコードから消えたことだけを根拠にした推定です。直ったことの証明ではありません。「再指摘なし・不明」は、今回は指摘されなかったが引用の行はまだある・ファイルが無い・引用が空（または、前回から根拠不一致だった）のいずれかです。',
        flowLabel: (title) => `${title} の流れ `,
        flowItem: (seq, failed, findings) => `${seq}（${failed ? '失敗' : `${findings} 件`}）`,
        flowNowTitle: 'いま開いているレビュー',
        flowOpenTitle: (when) => `${when} に実行。押すと、開きます`,
        panelTitle: (name) => `${name} からの変化`,
        panelTitleDefault: '前回のレビューからの変化',
        noTitle: '(見出しなし)',
        fateBefore: (stateLabel, file) => `（前回は「${stateLabel}」・${file}）`,
        fate: {
          persists: { badge: '継続', text: '今回も指摘されました（まだ直っていません）' },
          gone: { badge: '解消', text: '引用した行がコードから消えました（変更されたと見られます）' },
          not_reraised: { badge: '再指摘なし', text: '今回は指摘されませんでしたが、引用した行はまだコードにあります' },
          file_missing: { badge: 'ファイルなし', text: 'このファイルが見つかりません（削除・移動の可能性）' },
          unknown: { badge: '不明', text: '引用が空・もともと差分に見つからなかった引用などで、調べられませんでした' },
        },
        continueBtn: (name, findings) => `前回の続きから（${name}・${findings} 件）`,
        continueTitle: (when) => `${when} に実行したレビューの続きから、この観点を実行します`,
        continueFailed: (reason) => `前回の続きを調べられませんでした: ${reason}`,
      },
      resultEmpty: '上で観点を選んで実行するか、右の履歴を選ぶと、ここに結果（指摘・差分・記録）が出ます。',
      tabFindings: (count) => `指摘（${count}）`,
      tabDiff: '差分',
      tabRecord: '記録',
      reportThisReviewTitle: 'このレビューだけを、レポートにします',
      reportThisReviewBtn: 'このレビューをレポートに',
      backToHistory: '← 履歴・ログ',
      closeResult: '✕ 結果を閉じる',
      runLogDetail: {
        groupRun: '実行',
        groupInput: '入力',
        recipe: '観点',
        provider: 'プロバイダ・モデル',
        duration: '所要時間',
        tokens: 'トークン',
        tokensValue: (i, o) => `入力 ${i === null ? '報告なし' : i.toLocaleString()} / 出力 ${o === null ? '報告なし' : o.toLocaleString()}`,
        cost: '費用（提供元の報告）',
        failure: '失敗の理由',
        branch: 'ブランチ',
        base: '比較元',
        baseWorking: '未コミットの変更（HEAD との差）',
        baseCommit: (spec, subject) => (subject ? `${spec}（${subject}）から、いまの状態まで` : `${spec} から、いまの状態まで`),
        diff: '差分',
        diffValue: (files, bytes) => `${files.toLocaleString()} ファイル・${bytes.toLocaleString()} バイト`,
        specs: '設計書',
        none: 'なし',
        related: '関連コード',
        relatedValue: (n) => `${n.toLocaleString()} 件の断片を送りました`,
        prompt: 'プロンプト',
        promptValue: (chars) => `${chars.toLocaleString()} 文字`,
        loading: '読み込み中…',
        loadFailed: 'レビューの詳細を取得できませんでした。',
        notFound: 'レビューの記録が見つかりません。',
        diffSummary: 'レビューに送った差分',
        diffMissing: '差分を、記録から取り出せませんでした。',
      },
      viewDetailBtn: '詳細を開く',
      closeDetailBtn: '詳細を閉じる',
      recordSubDetail: '詳細',
      recordSubSent: '送った内容',
      recordSubResponse: '応答',
      execFailed: (err) => `実行に失敗: ${err}`,
      allowAndRerunBtn: '許可して再実行',
      parseFailed: (err) => `応答から指摘を読めませんでした: ${err}（「応答」タブに生の応答があります）`,
      noFindings: '指摘はありませんでした。',
      expandAllBtn: 'すべて開く',
      collapseAllBtn: 'すべて閉じる',
      fixAllSevBtn: (sev, n) => `${sev} を全て「対応」に（${n}）`,
      dismissAllSevBtn: (sev, n) => `${sev} を全て「無視」に（${n}）`,
      quoteVerified: '引用（差分に実在）',
      quoteNotVerified: '引用（この差分には見つからない）',
      noQuote: '(引用なし)',
      memoPlaceholder: '対応メモ（何をしたか・なぜ無視か。レポートに載ります）',
      markDoneBtn: '対応済みにする',
      doneRevertBtn: '✓ 対応済み（押すと戻す）',
      triageNoTimestamp: '仕分けた時刻: 記録なし（この機能より前の記録）',
      triageCarrySource: '前回のレビューから引き継ぎ',
      triageUserSource: 'あなたの操作',
      triageLastStamp: (whenStr, who, head7) => `最後に仕分けた時刻: ${whenStr}（${who}${head7 ? `・HEAD ${head7}` : ''}）`,
      idLabel: '識別子',
      copyBtn: 'コピー',
      copiedBtn: 'コピーしました',
      copyFailedBtn: 'コピーできませんでした',
      sendToCliBtn: 'CLI に渡す',
      writingCliBtn: '書き出しています…',
      sendToCliTitle: 'この指摘を、ファイルに書き出して、CLI に貼る 1 行をコピーします',
      sendAllToCliBtn: (count) => `対応 ${count} 件を、まとめて CLI に渡す`,
      viewAllHandoffSummary: (count) => `まとめて渡す内容を見る（対応 ${count} 件）`,
      copyHandoffTextBtn: 'この文面をコピー',
      mcpHint: '💡 agy や Claude Code からは MCP (mrcr mcp) で直接対応できます',
      fetchAgentBtn: '対応内容を取り込む',
      fetchingAgentBtn: '取り込んでいます…',
      fetchAgentTitle: 'エージェントが書いた「最後の報告」を、会話の記録から、取り込みます（`mrcr handoff report` で保存された分は、すでに入っています）',
      agentStateAll: (reported, total) => `対応内容: 保存済み（${reported}/${total}）`,
      agentStatePartial: (reported, total, missing) => `対応内容: 保存済み ${reported}/${total} ・未報告 ${missing} 件`,
      agentStateNone: (missing) => `対応内容: 未報告 ${missing} 件`,
      agentStateDecide: (skipped) => `対応しなかった ${skipped} 件（理由を読んで、仕分けるか、対応済みにしてください）`,
    },
  },

  en: {
    bcp47: 'en-US',
    listSeparator: ', ',
    noneParen: '(None)',
    autoLanguageLabel: 'Auto (System)',
    autoLanguageOptionLabel: 'Auto (System)',
    languageNames: {
      ja: 'Japanese',
      en: 'English',
    },

    triageChip: {
      doneText: 'Done',
      doneTitle: 'Resolved (counted in history)',
      openText: 'Open',
      openTitle: 'Pending fix',
    },

    specView: {
      toggleLabel: 'Refer to Spec / Requirements',
      tabFile: '📄 Select File',
      tabPaste: '📝 Paste Directly',
      cardTitle: '📘 Reference specification / design documents (one or more)',
      pickFile: '📄 Choose a file...',
      pickFileTitle: 'Opens the macOS file chooser',
      filePathPlaceholder: 'Type a file path (e.g. docs/spec.md) and press Enter, or add with "Choose a file..."',
      clearSelectionTitle: 'Clear the selection',
      pasteAreaPlaceholder: 'Paste requirements from a GitHub Issue, a PR description, acceptance criteria, or the text of a design document here...',
      clearInputTitle: 'Clear the input',
      fileSelectPlaceholder: 'Select file (.md / .txt)',
      noFilesFound: 'No specification files (.md / .txt) found',
      allSections: '(Entire file)',
      sectionSelectPlaceholder: 'Select section / heading',
      pastePlaceholder: 'Paste requirements from GitHub Issues, Notion, or spec notes...',
      charCount: (chars, tokens) => `${chars.toLocaleString()} chars (~${tokens.toLocaleString()} tokens)`,
      charCountOver: (chars, max) => `⚠ Limit exceeded: ${chars.toLocaleString()} / max ${max.toLocaleString()} chars (narrow down sections or shorten)`,
      clearButton: 'Clear',
      specBadge: 'Spec Match',
      specQuoteLabel: 'Specification Requirement',
      specUnverifiedBadge: 'Quote Not Found in Spec',
      specVerifiedBadge: 'Verified against Spec',
      addedCount: (n, max) => `${n} / ${max} files selected`,
      removeFile: 'Remove',
      maxFilesReached: (max) => `Up to ${max} spec files can be used`,
    },

    recipeSummaryMap: {
      bugs: 'Find patterns that cause runtime bugs (boundaries, null, concurrency, etc.)',
      errors: 'Find swallowed errors, missed exceptions, and overlooked failures',
      security: 'Find security holes in input validation, auth, and secret handling',
      performance: 'Find redundant loops, expensive operations, N+1 queries, etc.',
      tests: 'Find missing test coverage and weak assertions',
      consistency: 'Find inconsistencies between changed parts and untouched code',
      spec: 'Check the diff against a specification or requirements doc for contradictions',
    },
    recipeGroupLlm: 'Review with LLM',
    recipeGroupTool: 'Without LLM',
    recipeBadgeRepo: 'This repo',

    formatRange: (min, max, unit) => `${min} - ${max}${unit}`,
    groupNames: {
      '送る差分': 'Diff to Send',
      '関連コード': 'Related Code',
      '実行': 'Execution',
      '仕分けの引き継ぎ': 'Triage Carry-over',
      '画面': 'Display',
    },
    settingMeta: {
      max_file_bytes: {
        label: 'Max file size in diff',
        description: 'Files exceeding this size in the diff are excluded from review.',
        why: 'Prevents large generated files or binaries from exhausting token limits.',
        when: 'Increase if you need to review large source files.',
        unit: ' bytes',
      },
      max_total_bytes: {
        label: 'Max total diff size',
        description: 'Total size limit of the diff sent to the LLM.',
        why: 'Avoids overflowing model context windows and request timeouts.',
        when: 'Increase for large refactorings or comprehensive changes.',
        unit: ' bytes',
      },
      max_untracked_files: {
        label: 'Max untracked files',
        description: 'Maximum number of newly added (untracked) files to include.',
        why: 'Prevents accidentally sending massive numbers of untracked files.',
        when: 'Adjust when adding many new files at once.',
        unit: ' files',
      },
      ignored_lockfiles: {
        label: 'Ignored lockfiles',
        description: 'Lockfiles excluded from diff (package-lock.json, Cargo.lock, etc.).',
        why: 'Generated lockfile changes consume excessive tokens without review value.',
        when: 'Add custom lockfile filenames used in your projects.',
      },
      carry_window: {
        label: 'Triage carry-over window',
        description: 'Number of past successful reviews to inspect when inheriting triage decisions.',
        why: 'Carries over your "Fix", "Wait", and "Dismiss" decisions from recent reviews.',
        when: 'Increase if you iterate frequently and want older decisions remembered.',
        unit: ' reviews',
      },
      fuzzy_min: {
        label: 'Fuzzy triage match threshold',
        description: 'Similarity threshold for carrying over triage when code shifted slightly.',
        why: 'Prevents wrong matches while tolerating minor line shifts.',
        when: 'Lower to carry over across larger edits, or raise to be stricter.',
      },
      fuzzy_min_tokens: {
        label: 'Min tokens for fuzzy triage matching',
        description: 'Minimum token count required in evidence line for fuzzy matching.',
        why: 'Short lines like single braces are too common to match fuzzily.',
        when: 'Lower if you want shorter statements to inherit triage.',
        unit: ' tokens',
      },
      llm_timeout_secs: {
        label: 'LLM request timeout',
        description: 'Timeout duration for AI review requests.',
        why: 'Prevents infinite UI hangs on network issues or slow provider responses.',
        when: 'Increase if reviews on large diffs frequently time out.',
        unit: 's',
      },
      related_max_bytes: {
        label: 'Max related code context size',
        description: 'Limit on related code outside the diff attached to cross-file reviews.',
        why: 'Provides call-site context without overwhelming prompt token budgets.',
        when: 'Increase if consistency reviews need wider caller context.',
        unit: ' bytes',
      },
      related_max_names: {
        label: 'Max symbol names searched for context',
        description: 'Maximum number of changed functions/types to search call sites for.',
        why: 'Focuses search on primary changed symbols.',
        when: 'Increase if many symbols were modified.',
        unit: ' symbols',
      },
      related_hits_per_name: {
        label: 'Max call sites per symbol',
        description: 'Maximum usage snippets included per symbol name.',
        why: 'Gives representative usage samples without repetitive snippets.',
        when: 'Increase to see more callers.',
        unit: ' hits',
      },
      related_context_lines: {
        label: 'Context lines around call site hits',
        description: 'Number of surrounding lines shown around each call site hit.',
        why: 'Provides enough context to understand caller invocation.',
        when: 'Increase to see larger caller functions.',
        unit: ' lines',
      },
      related_skip_over_hits: {
        label: 'Skip symbols with too many hits',
        description: 'Threshold above which common symbols are excluded from caller search.',
        why: 'Common utility names appear everywhere and would clutter context.',
        when: 'Adjust threshold for widespread helper names.',
        unit: ' hits',
      },
      history_limit: {
        label: 'Review history display count',
        description: 'Number of recent reviews listed in the history sidebar.',
        why: 'Fast list generation while retaining older reviews on disk.',
        when: 'Increase to browse further back in history.',
        unit: ' items',
      },
      language: {
        label: 'Language',
        description: 'Display language and built-in review angle prompt language (auto: follow OS).',
        why: '"Auto" matches your operating system language naturally.',
        when: 'Change to English or Japanese to fix the language.',
      },
    },
    settingsTabs: [
      { id: 'llm', label: 'LLM & API Key', desc: 'LLM used for reviews and BYOK (Bring Your Own Key).', groups: [] },
      { id: 'repos', label: 'Repositories', desc: 'Manage registered repositories (display names, order, unlinking).', groups: [] },
      { id: 'diff', label: 'Diff & Context', desc: 'Limits on diff size and related code sent to the LLM.', groups: ['送る差分', '関連コード'] },
      { id: 'run', label: 'Execution', desc: 'Timeouts and execution limits for LLM and tools.', groups: ['実行'] },
      { id: 'triage', label: 'Triage', desc: 'Carry-over of triage decisions from the previous review.', groups: ['仕分けの引き継ぎ'] },
      { id: 'display', label: 'Display', desc: 'History display count and display language.', groups: ['画面'] },
      { id: 'link', label: 'Integration', desc: 'Agent registration and consent to read conversation logs.', groups: ['連携'] },
      { id: 'hook', label: 'Git Hook', desc: 'Run a review automatically after each commit.', groups: [] },
    ],
    otherTab: { id: 'other', label: 'Other', desc: 'Settings not in other tabs', groups: [] },
    settingsUi: {
      title: 'Settings',
      intro1: 'Initial values are recommended standards. Customize only what you need. Changed items are marked and can be reset anytime.',
      introFile: (path, exists) =>
        `Settings file: ${path} (${exists ? 'exists' : 'not created yet; created on save'}). Only custom settings are stored.`,
      repoInvalid: (path, err) => `Invalid repository settings file (${path}): ${err}`,
      repoOverrides: (path, keysText) => `Repository settings (${path}) overrides: ${keysText}.`,
      fileInvalid: (err) => `Invalid settings file: ${err}. Opened with recommended defaults.`,
      save: 'Save',
      saving: 'Saving…',
      undo: 'Undo changes',
      resetAll: 'Reset all to recommended',
      close: 'Close',
      saveAndClose: 'Save and close',
      discardAndClose: 'Discard and close',
      keepEditing: 'Keep editing',
      validationErrorsCount: (n) => `${n} validation error(s)`,
      unsavedChangesCount: (n) => `${n} unsaved change(s)`,
      tallyText: (changed, dirty, errors) =>
        `${changed} customized` +
        (dirty ? ` · ${dirty} unsaved` : '') +
        (errors ? ` · ${errors} error(s)` : ''),
      confirmUnsaved: (n) => `You have ${n} unsaved change(s). Save before closing?`,
      cannotSaveWithErrors: 'Cannot save due to validation errors. Fix them or choose "Discard and close".',
      onePerLine: 'One per line',
      customizedBadge: 'Customized',
      resetToDefault: 'Reset to default',
      overriddenByRepo: 'Overridden by repository',
      canOverrideByRepo: 'Can be overridden by repository',
      personalSettingOnly: 'Personal setting only',
      defaultLabel: (val) => `Default: ${val}`,
      rangeLabel: (range) => `(Range: ${range})`,
      whySummary: 'Why recommended & when to change',
      whyPrefix: 'Why: ',
      whenPrefix: 'When: ',
      savedToast: 'Settings saved.',
      savedWithErrors: (errs) => `Settings saved, but failed to apply some changes: ${errs.join(' / ')}`,
    },

    agentConsent: {
      title: 'Agent Integration (Reading Conversation Logs)',
      desc: '"Import Agent Response" searches Claude Code, agy and GitHub Copilot CLI conversation logs for the final response line written by the agent. Full conversation text is never saved.',
      checking: 'Checking consent status…',
      consentedStrong: 'Consented',
      consentedBody: '. Clicking "Import Agent Response" reads conversation logs without asking for confirmation.',
      revokeBtn: 'Revoke Consent',
      revokedNote:
        'Imported responses remain even after revoking. You will be asked for permission again next time you click "Import Agent Response".',
      notConsentedStrong: 'Not Consented',
      notConsentedBody: ' (not yet granted or revoked). You will be asked for permission the first time you click "Import Agent Response".',
    },

    hookUi: {
      tabLabel: 'Git Hook',
      title: 'Automatic review after each commit',
      desc: 'Each time you commit in this repository, the commit you just made is reviewed automatically. Results go into the normal review history. Commits are never blocked (blocking is not available yet).',
      noRepo: 'Select a repository on the left to configure it.',
      loading: 'Reading the current state…',
      stateOn: 'ON (reviews after every commit)',
      stateOff: 'OFF',
      enable: 'Turn ON',
      disable: 'Turn OFF',
      switchLabel: 'Automatic review after each commit',
      busy: 'Working…',
      recipesLabel: 'Review recipes',
      recipesNote: 'When you choose a recipe marked "needs a spec", specify the spec document below.',
      saveRecipes: 'Save changes',
      noRecipes: 'No recipes available.',
      needsSpecBadge: 'needs a spec',
      specLabel: 'Spec documents (passed to recipes that need one; one per line, up to 5)',
      specHint: 'Files inside this repository, as paths relative to the repository (.md / .txt; sections cannot be specified). Files outside the repository cannot be used.',
      specPlaceholder: 'e.g.\ndocs/requirements.md\ndocs/design.md',
      groupCode: 'Code',
      groupTf: 'Terraform (tfstate)',
      groupDoc: 'Documents',
      groupOther: 'This repository / your own recipes',
      groupCodeDesc: 'Recipes for code changes. Checking against a spec (spec) is here too.',
      groupTfDesc: 'Recipes for Terraform changes (.tf etc.). Not needed for code-only diffs.',
      groupDocDesc: 'Recipes for changes to documents (README, specs, design notes).',
      groupOtherDesc: 'Recipes placed in .mrcodereview/recipes of this repository, or added by you.',
      selectAll: 'Select all',
      clearAll: 'Clear all',
      groupCount: (picked, total) => `${picked} / ${total} selected`,
      installedYes: 'The hook is installed in this repository.',
      installedNo: 'The hook is not installed in this repository.',
      cliMissing: (brewCmd, npmCmd) =>
        `The mrcr CLI was not found. The hook calls the CLI, so install it first (e.g. ${brewCmd} or ${npmCmd}). A copy started with npx lives in a temporary cache and disappears later, so it cannot be used.`,
      targetMissing:
        'The mrcr that the hook calls was not found (it may have been updated or removed). Reinstall mrcr, then turn the hook OFF and ON again.',
      cliPath: (path) => `mrcr called by the hook: ${path}`,
      cliWillUse: (path) => `When turned ON, the hook will call mrcr: ${path}`,
      cliDiffers: (path) => `The mrcr found now is ${path}. Turn the hook OFF and ON again to switch to it.`,
      blocked: (why) => `The hook cannot be installed automatically in this repository: ${why}`,
      costNote: 'Every commit uses the LLM API (pay-as-you-go).',
      keyNote:
        'The mrcr called from the hook uses the ANTHROPIC_API_KEY environment variable. If you use git from a terminal it is inherited. If it is not visible, the run is shown below as a failure (the commit is never blocked).',
      lastRunTitle: 'Run log (newest first)',
      lastRunNone: 'Not run yet.',
      refresh: 'Refresh',
      refreshFailed: (err) => `Could not read the latest state: ${err}`,
      logUnreadable: (n) => `The run log has ${n} unreadable line(s) (including other repositories').`,
      runCommit: (commit, subject) => `Commit ${commit} "${subject}"`,
      runNoCommit: '(no commit recorded)',
      runKind: 'Hook',
      runState: (state) => (state === 'running' ? 'Running' : state === 'done' ? 'Done' : state === 'failed' ? 'Failed' : 'Aborted'),
      runSummary: (findings, secs) => [findings === null ? '' : `${findings} findings`, secs ?? ''].filter(Boolean).join(' · '),
      stepOk: (findings) => `${findings} findings`,
      stepFailed: 'Failed',
      stepRunning: 'Running…',
      secs: (ms) => `${(ms / 1000).toFixed(1)} s`,
      failed: (err) => `The operation failed: ${err}`,
    },

    repoView: {
      emptyTitle: 'No registered repositories.',
      emptyHint: 'Register a repository using "+ Add".',
      appIconTitle: (title) => `${title} app icon`,
      renameTitle: 'Rename',
      setNameTitle: 'Set display name',
      unlinkTitle: 'Unlink registration',
      openCountTitle: (n) => `${n} open finding(s) (untriaged or in progress; not counting Wait, Ignore or Done)`,
      branchTitle: 'The branch checked out now',
      colRepo: 'Repository',
      colBranch: 'Branch',
      colLast: 'Last review',
      colHook: 'Git hook',
      colOpen: 'Open',
      lastReview: (when) => `Last review ${when}`,
      noReview: 'No review yet',
      hookOn: 'Hook ON',
      hookOff: 'Hook OFF',
      hookCheck: 'Hook: check',
      hookOnTitle: 'Reviews run automatically after each commit (Git hook)',
      hookOffTitle: 'The Git hook is not used (you can install it from Settings)',
      hookCheckTitle: 'The setting is ON but the hook is not installed. Reinstall it under Settings > Git hook',
      heading: 'Repositories',
      addTitle: 'Add repository',
      addBtn: 'Add',
    },

    recipeBar: {
      groupDocs: 'Documents',
      runningLabel: (progress) => `Running…${progress}`,
      selectAngles: 'Select review angles',
      runSingle: (title) => `Run "${title}"`,
      runFallback: 'Run',
      runMultiple: (count) => `Run ${count} selected angles`,
      runSkipping: (count, what) => `Run ${count} (excluding ${what})`,
      skipWhatSpec: 'spec-based angles',
      skipWhatLlm: 'angles that need an LLM',
      skipWhatMixed: 'angles that cannot run',
      skipBadge: 'Not running',
      skipNotice: (titles, reason) => `Not running ${titles.map((t) => `"${t}"`).join(', ')} this time. ${reason}`,
      lastReviewNote: (titles) => `${titles.map((t) => `"${t}"`).join(', ')}: not part of the last run, so changes made before it are not reviewed.`,
      expandNewReview: '▸ New review',
      showAllRecipes: (hidden) => `Show all angles (${hidden} more)`,
      showFitOnly: 'Only angles that fit this diff',
      selectAll: 'Select All',
      clearAll: 'Deselect All',
      estTokens: (tokens) => `Est. input ~${tokens} tokens`,
      estTime: (time) => `Est. time ${time}`,
      cancel: 'Cancel',
    },


    baseInfo: {
      uncommitted: 'Uncommitted changes',
      entireRepo: 'Entire repository (from start)',
      fromCommit: (hash7, subject) => `From ${hash7} ${subject}`,
      scopeSuffix: ' (committed, uncommitted, untracked)',
      onlyUncommitted: 'Only uncommitted changes',
      lastCommitAndUncommitted: 'Last commit and uncommitted changes',
      allChangesFromForward: (name, scope) => `All changes from ${name} forward${scope}`,
      allChangesAfterBranch: (branch, scope) => `All changes after branching from ${branch}${scope}`,
      allChangesFromBase: (label, scope) => `All changes from ${label.replace(/^From /i, '')}${scope}`,
      groupRange: 'Select Range',
      groupRecent: 'From Recent Commits',
      groupOther: 'Other',
      latestCommitTitle: 'Latest commit',
      entireRepoHint: 'From the initial commit to current state. If large, limits may truncate.',
      fromCustomTitle: (spec) => `From ${spec}`,
      customDesc: 'Custom location',
      customItemTitle: 'Custom location…',
      customItemDesc: 'Enter commit, branch, or tag name',
      lastReviewTitle: (when) => `Since the last review (${when})`,
      lastReviewDesc: (short, ran) => `From ${short} to the current working tree. Last run: ${ran}. Changes that were uncommitted at the last review appear again.`,
      allChangesSinceLastReview: (when, scope) => `All changes since the last review (${when})${scope}`,
      translateBaseHint: (desc) => {
        if (desc.includes('最後のコミットと、コミットしていない変更')) {
          return desc.replace('（最後のコミットと、コミットしていない変更）', ' (Last commit and uncommitted changes)');
        }
        if (desc.includes('このコミット以降すべて（このコミットを含む）')) {
          return desc.replace(
            'このコミット以降すべて（このコミットを含む）',
            'All changes from this commit forward (including this commit)',
          );
        }
        return desc;
      },
      normalizeBaseTitle: (value, label) => {
        if (value === 'HEAD' || label === '未コミットの変更') {
          return { title: 'Uncommitted changes', desc: 'Only uncommitted changes' };
        }
        if (value === 'HEAD~1' || label === '直近のコミット') {
          return { title: 'Latest commit' };
        }
        if (value === ':root' || label === '最初から（リポジトリ全体）') {
          return {
            title: 'Entire repository (from start)',
            desc: 'From the initial commit to current state. If large, limits may truncate.',
          };
        }
        return null;
      },
    },

    builtinRecipeTitles: {
      bugs: 'Latent Bugs',
      '潜在バグ': 'Latent Bugs',
      '潜在的な不具合': 'Latent Bugs',
      errors: 'Error Handling',
      '例外処理': 'Error Handling',
      security: 'Security',
      'セキュリティ': 'Security',
      performance: 'Performance',
      'パフォーマンス': 'Performance',
      tests: 'Missing Tests',
      'テストの抜け': 'Missing Tests',
      consistency: 'Cross-file Inconsistencies',
      '他ファイルとの不整合': 'Cross-file Inconsistencies',
      'tf-state': 'State & Addresses',
      '状態・アドレス': 'State & Addresses',
      'tf-replace': 'Replacement & Destruction',
      '置換・破壊的変更': 'Replacement & Destruction',
      'tf-repro': 'Reproducibility & Versions',
      '再現性・バージョン': 'Reproducibility & Versions',
      'tf-security': 'Access, Exposure & Secrets',
      '権限・公開・秘密': 'Access, Exposure & Secrets',
      'tf-refs': 'Reference Consistency',
      '参照の整合': 'Reference Consistency',
      'doc-core': 'Core & Necessity',
      '文書の核': 'Core & Necessity',
      'doc-claims': 'Unsupported Claims & Numbers',
      '根拠のない主張・数字': 'Unsupported Claims & Numbers',
      'doc-reality': 'Reality & Currency Check',
      '実在性・最新性の要確認': 'Reality & Currency Check',
      'doc-steps': 'Gaps in Steps, Assumptions & Decisions',
      '手順・前提・決定の抜け': 'Gaps in Steps, Assumptions & Decisions',
      lint: 'Linters',
      'リンター': 'Linters',
    },
    historyView: {
      formatDay: (d) => d.toLocaleDateString('en-US', { year: 'numeric', month: 'short', day: 'numeric' }),
      countUnit: (label, n) => `${label} ${n}`,
      countsTitle: 'Open = Untriaged / Fix, Done = Fixed / Agent reported done, Wait = Waiting, Dismiss = Dismissed',
      countOpen: 'Open',
      countDone: 'Done',
      countWait: 'Wait',
      countDismiss: 'Dismiss',
      selectForReportAria: 'Select review for report',
      failedBadge: 'Failed',
      detailTitle: 'Open review details (duration, tokens, diff range, etc.)',
      detailBtn: 'Details',
      findingsCount: (n) => `${n} finding(s)`,
      showMore: (n) => `Show more (${n} left)`,
      groupCount: (n) => ` (${n})`,
      expandHistory: 'Expand history',
      collapseHistory: 'Collapse history',
      heading: 'History',
      deselectAll: 'Deselect all',
      selectAll: 'Select all',
      selectedCount: (n) => `${n} selected`,
      exportReportBtn: 'Export to Report',
      deleteBtn: 'Delete selected',
      deleteConfirm: (n) => `Delete ${n} selected review(s)?\n(Triage, notes and agent reports go with them. This cannot be undone.)`,
      deleteOk: 'Delete',
    },

    usageView: {
      periods: { today: 'Today', week: '7 days', all: 'All time' },
      colReviews: 'Reviews',
      colInput: 'Input',
      colOutput: 'Output',
      failedSuffix: (n) => ` (${n} failed)`,
      unreportedTitle: 'Some reviews did not report tokens and are excluded',
      unreportedSuffix: (n) => ` +${n} unreported`,
      noRecords: 'No records found for this period.',
      heading: 'Token Usage',
      subtitle: 'Sums measured values from recorded reviews only.',
      closeBtn: 'Close',
      calculating: 'Calculating…',
      cardInput: 'Input',
      cardOutput: 'Output',
      cardCacheRead: 'Cache Read',
      cardCacheWrite: 'Cache Write',
      totalReviews: (n) => `${n} reviews`,
      totalFailedExcluded: (n) => ` (${n} failed reviews excluded)`,
      totalUnreportedExcluded: (n) => ` · ${n} reviews without token reports (excluded)`,
      unreadableError: (n) => `${n} unreadable record(s) could not be loaded and are excluded from this summary.`,
      reportedCost: (usd, count) => `Reported cost by provider (reference): $${usd} (${count} reviews. Not billed by subscription)`,
      byAngle: 'By Review Angle',
      byRepo: 'By Repository',
      byModel: 'By LLM & Model',
      footnote:
        'Reviews that do not use an LLM (such as past Linter runs) do not call one, so their token count is 0 (measured 0, not unreported). Missing values from providers are not filled with estimates.',
    },

    reportView: {
      periods: { today: 'Today', week: '7 days', month: '30 days', all: 'All time', custom: 'Custom range' },
      invalidStartDate: 'Invalid start date',
      invalidEndDate: 'Invalid end date',
      startAfterEnd: 'Start date must be before end date',
      fileNameSelected: (count, date, ext) => `review_report_selected_${count}_${date}.${ext}`,
      fileNameAll: 'all',
      fileNameRepo: (safeRepo, date, ext) => `review_report_${safeRepo}_${date}.${ext}`,
      heading: 'Report',
      subtitle: 'Export review records and triage actions as deliverables.',
      pickedTargeting: (count) => `Targeting ${count} review(s) selected from History (date range and repo settings are bypassed).`,
      switchToDateRange: 'Switch to date range',
      closeBtn: 'Close',
      dateRangeSep: ' – ',
      selectedRepoBtn: 'Selected repository',
      allReposBtn: 'All repositories',
      includeEvidence: 'Include code snippets',
      includeTokens: 'Include token usage',
      includeLog: 'Include triage history',
      includeLogHint: '(timeline of actions)',
      includePrompt: 'Include full prompt text sent to LLM',
      includePromptHint: '(increases size)',
      secretNotice:
        'Before including in deliverables, review the preview to ensure that sensitive values (keys, passwords, etc.) are not exposed in quoted code.',
      loading: 'Loading…',
      summaryLine: (reviews, findings, fix, wait, dismiss, fresh) =>
        `${reviews} review(s) · ${findings} finding(s) (Fix: ${fix}, Wait: ${wait}, Dismiss: ${dismiss}, New: ${fresh})`,
      savingBtn: 'Saving…',
      saveMdBtn: 'Save as Markdown',
      saveXlsxBtn: 'Save as Excel',
      printTitle: 'Opens the print dialog. Choose "PDF" → "Save as PDF".',
      openingPrintBtn: 'Opening print dialog…',
      exportPdfBtn: 'Export as PDF',
      generatingPreview: 'Generating preview…',
    },

    mainUi: {
      repoDataSummary: (d) =>
        d.reviews === 0
          ? 'No history'
          : `History: ${d.reviews} · Findings: ${d.findings} · Triaged: ${d.triaged} · Memos: ${d.memos}`,
      repoDeleteConfirmTitle: (title) => `Delete the history and data for "${title}".`,
      repoDeleteConfirmBody: (summary) => `Data to be deleted: ${summary}. Repository files will not be deleted. `,
      repoDeleteCannotUndo: 'Cannot be undone.',
      repoDeleteExportHiddenHint: 'To export, first "Restore" and generate a report.',
      repoDeleteExportVisibleHint: 'You can export to a report (Markdown, Excel, PDF) first.',
      openReportBtn: 'Open Report',
      deleteAndFreeSlotBtn: 'Delete',
      cancelBtn: 'Cancel',
      deleteDataTitle: 'Delete history and data',
      deleteDataBtn: 'Delete data…',
      displayNameAria: (title) => `Display name of ${title}`,
      inUseChip: 'In Use',
      subdirNote: (root) => `Subdirectory registered (repository root: ${root}). History is associated with the root path.`,
      moveUpTitle: 'Move up',
      moveDownTitle: 'Move down',
      unlinkWithHistoryTitle: 'Unlink from list (history is kept; "Restore" brings it back)',
      unlinkBtn: 'Unlink',
      restoreBtn: 'Restore',
      removeEmptyTitle: 'No history. Remove the registration.',
      removeEmptyBtn: 'Remove registration',
      registeredReposHeading: 'Registered Repositories',
      registeredReposIntro1: (limit) =>
        limit === null
          ? 'There is no limit on how many repositories you can register. "Unlink" only removes a repository from the active list; its history is kept ("Restore" brings it back; registrations with no history are removed completely). To delete the history too, use "Delete data…".'
          : `You can use data for up to ${limit} repositories. "Unlink" only removes a repository from the active list; slots remain occupied while history exists (registrations with 0 history entries are removed completely, freeing the slot). To free a slot, use "Delete data…".`,
      registeredReposIntro2:
        'Display name defaults to the folder name if left blank. The path cannot be changed because history is linked to it. If you moved the repository, register the new location.',
      slotsLabel: 'Registered: ',
      slotsUnlimited: (used) => `${used} (no limit)`,
      slotsOverLimit: 'Limit reached (new additions paused; existing repositories work normally)',
      slotsFull: 'All slots in use',
      addRepoBtn: '+ Add Repository…',
      noReposRegistered: 'No repositories registered.',
      unlinkedHoldingSlots: (count) => `Unlinked (with history): ${count}`,
      savedReportToast: (path) => `Saved: ${path}`,
      rangePrefix: 'Range',
      baseInputPlaceholder: 'Commit, branch, or tag name (e.g. v1.2.0, feature/x, abc1234)',
      baseFormHint: 'Shows changes from here to the working tree',
      baseFormSetBtn: 'Set',
      baseFormCancelBtn: 'Cancel',
      pageTabsLabel: 'Pages',
      pageTabRepos: 'Repositories',
      pageTabHistory: 'History & log',
      hookRunsHeading: 'Hook runs',
      branchTitle: 'The branch checked out now',
      historySearchPlaceholder: 'Filter (space-separated words must all match. e.g. bugs yesterday open)',
      historySearchLabel: 'Filter history and log',
      historySearchCount: (shown, total) => `${shown} of ${total}`,
      historySearchClear: 'Clear the filter',
      historySearchNoMatch: 'Nothing matches. Remove a word or clear the filter.',
      runRecordSummary: 'Run record (model, tokens, duration, input, diff)',
      pageTabNewReview: 'New review',
      usageTitle: 'Token usage',
      usageBtn: 'Usage',
      reportTitle: 'Review report',
      reportBtn: 'Report',
      settingsTitle: 'Settings (⌘,)',
      settingsBtn: 'Settings',
      llmHeading: 'LLM Provider (BYOK / Local Settings)',
      llmSubtitle: 'Select the LLM model or CLI provider used for reviews.',
      byokKeyPlaceholder: 'sk-ant-… (Anthropic API Key)',
      byokLoading: 'Loading…',
      byokUnsupported: 'Key storage is not supported on this machine (macOS Keychain only)',
      byokVerifyErr: (err) => `Cannot verify key: ${err}`,
      byokConfigured: (last4) => `Configured (ending in ${last4})`,
      byokNotConfigured: 'Not configured',
      byokHeading: 'Anthropic API Key (BYOK)',
      byokNote:
        'The key is stored only in macOS Keychain (never in settings files, history, or logs). Once saved, select "Anthropic API" under LLM above to use it. Review contents (diffs and angle descriptions) are sent to api.anthropic.com.',
      byokKeyLabel: 'Key: ',
      byokReplaceBtn: 'Replace',
      byokSaveBtn: 'Save',
      byokConfirmDeleteBtn: 'Confirm delete',
      byokDeleteBtn: 'Delete',
      byokModelLabel: 'Model ',
      byokMaxTokensLabel: 'Max output tokens ',
      byokInvalidConfig: (err) => `Invalid byok.json (showing defaults): ${err}`,
      recipeInfoToolTitle: (title) => `${title} (no LLM)`,
      recipeInfoRepoSource: ' (repo setting)',
      recipeInfoSummary: (count) => `View angle prompts (${count} prompt${count > 1 ? 's' : ''} to LLM)`,
      sendBoxTitleTool: 'Target Diff',
      sendBoxTitleLlm: 'Diff to send',
      sendBoxCannotRead: 'Cannot read',
      sendBoxSummaryTool: (title, count) => `${title}: ${count} file(s) (not sent to LLM)`,
      sendBoxSummaryLlm: (title, count, tokens, excluded) =>
        `${title}: ${count} file(s) · ~${tokens} tokens${excluded ? ` · Excluded: ${excluded}` : ''}`,
      sendBoxRangeLabel: 'Range: ',
      sendBoxDiffSep: ' / Diff ',
      sendBoxBytesSuffix: (bytes) => ` / ${bytes} bytes (tokens estimated)`,
      sendBoxExcludedHeading: (count) => `Excluded: ${count}`,
      sendBoxNoDiff: 'No diff (no changes from HEAD).',
      sendBoxSelectRepoSummary: (title) => `${title}: Select a repository to preview`,
      sendBoxSelectRepoBody: 'Select a repository to see which files are sent/excluded and estimated tokens.',
      rightPanelAllRepos: 'All repositories',
      rightPanelEmptyRepo: 'No review history for this repository.',
      rightPanelEmptyAll: 'No history yet.',
      triageLabels: [
        ['fix', 'Fix'],
        ['wait', 'Wait'],
        ['dismiss', 'Dismiss'],
      ],
      triageResetTitle: 'Click again to reset to new',
      chipEvidenceMismatch: '⚠ Evidence mismatch',
      chipEvidenceMismatchTitle: () => 'Quoted line was not found in this diff.',
      chipSpecVerified: '📋 Spec Verified',
      chipSpecMismatch: '⚠ Spec Mismatch',
      specQuoteVerified: '📋 Specification Quote (Verified)',
      specQuoteMismatch: '⚠ Specification Quote (Mismatch)',
      specQuoteLabel: '📋 Specification Quote',
      specQuoteSource: (name) => `Source: ${name}`,
      chipAgentDid: 'Agent: Did',
      chipAgentSkipped: 'Agent: Skipped',
      chipAgentTitle: 'Reported by coding agent',
      chipMemo: '📝 Memo',
      chipCarried: '↺ Carried',
      chipCarriedTitle: (_stateLabel, stateRaw) => `Carried previous status: ${stateRaw}`,
      chipNew: 'New',
      chipNewTitle: 'Not in the previous review',
      chipPersist: (no) => `Still open (prev #${no})`,
      chipPersistTitle: 'Judged to be the same finding as in the previous review. Click to jump to it',
      handoffHint: 'Mark a finding as "Fix" and a "Send to CLI" button appears next to it.',
      runPlan: {
        noRepo: 'Select a repository (left sidebar)',
        noLlm: 'Select an LLM (from Settings, top right)',
        noSpec: 'Specify a specification document (choose a file or paste text)',
        badSpec: 'Check the specification document (it exceeds the size limit or is invalid)',
        noDiffUncommitted: 'Nothing to send (no uncommitted changes)',
        noDiffRange: 'Nothing to send (no changes in this range)',
      },
      continuity: {
        stateLabel: { new: 'Unsorted', fix: 'Fix', wait: 'Wait', dismiss: 'Dismiss' },
        carryNote: (stateLabel, exact) => `Carried the previous status "${stateLabel}"` + (exact ? '' : ' (quote nearly identical)'),
        prev: 'Previous ',
        prevTitle: (when) => `Run at ${when}. Click to open it`,
        prevRun: (name, findings) => `${name} (${findings} ${findings === 1 ? 'finding' : 'findings'})`,
        thisRun: (name, findings) => `This run ${name} (${findings} ${findings === 1 ? 'finding' : 'findings'})`,
        persist: 'Still open',
        gone: 'Resolved',
        fresh: 'New',
        other: 'Not re-raised / unknown',
        chipOnTitle: 'Click again to remove the filter',
        chipOffTitle: 'Show only this kind',
        note: '"Resolved" is only an estimate: the quoted line is no longer in the code. It is not proof that the problem was fixed. "Not re-raised / unknown" means the finding was not raised this time but the quoted line is still there, the file is missing, the quote is empty, or the quote was not found in the diff in the first place.',
        flowLabel: (title) => `${title}: `,
        flowItem: (seq, failed, findings) => `${seq} (${failed ? 'failed' : findings})`,
        flowNowTitle: 'The review that is open now',
        flowOpenTitle: (when) => `Run at ${when}. Click to open it`,
        panelTitle: (name) => `Changes since ${name}`,
        panelTitleDefault: 'Changes since the previous review',
        noTitle: '(no title)',
        fateBefore: (stateLabel, file) => `(was "${stateLabel}" · ${file})`,
        fate: {
          persists: { badge: 'Still open', text: 'Raised again this time (not fixed yet)' },
          gone: { badge: 'Resolved', text: 'The quoted line is gone from the code (it seems to have been changed)' },
          not_reraised: { badge: 'Not re-raised', text: 'Not raised this time, but the quoted line is still in the code' },
          file_missing: { badge: 'File missing', text: 'This file cannot be found (it may have been deleted or moved)' },
          unknown: { badge: 'Unknown', text: 'Could not be checked (for example, the quote is empty or was never found in the diff)' },
        },
        continueBtn: (name, findings) => `Continue from previous (${name}, ${findings} ${findings === 1 ? 'finding' : 'findings'})`,
        continueTitle: (when) => `Runs this angle from the review run at ${when}`,
        continueFailed: (reason) => `Could not check the previous run: ${reason}`,
      },
      resultEmpty:
        'Select review angles above to run, or choose a review from History on the right to view results (findings, diff, logs).',
      tabFindings: (count) => `Findings (${count})`,
      tabDiff: 'Diff',
      tabRecord: 'Record',
      reportThisReviewTitle: 'Create report for this review',
      reportThisReviewBtn: 'Report this review',
      backToHistory: '← History & log',
      closeResult: '✕ Close the result',
      runLogDetail: {
        groupRun: 'Run',
        groupInput: 'Input',
        recipe: 'Recipe',
        provider: 'Provider / model',
        duration: 'Duration',
        tokens: 'Tokens',
        tokensValue: (i, o) => `in ${i === null ? 'not reported' : i.toLocaleString()} / out ${o === null ? 'not reported' : o.toLocaleString()}`,
        cost: 'Cost (as reported)',
        failure: 'Why it failed',
        branch: 'Branch',
        base: 'Compared from',
        baseWorking: 'Uncommitted changes (against HEAD)',
        baseCommit: (spec, subject) => (subject ? `${spec} (${subject}) to the current state` : `${spec} to the current state`),
        diff: 'Diff',
        diffValue: (files, bytes) => `${files.toLocaleString()} files · ${bytes.toLocaleString()} bytes`,
        specs: 'Design docs',
        none: 'None',
        related: 'Related code',
        relatedValue: (n) => `${n.toLocaleString()} snippets sent`,
        prompt: 'Prompt',
        promptValue: (chars) => `${chars.toLocaleString()} chars`,
        loading: 'Loading…',
        loadFailed: 'Could not load the review details.',
        notFound: 'The review record was not found.',
        diffSummary: 'The diff sent for review',
        diffMissing: 'The diff could not be read from the record.',
      },
      viewDetailBtn: 'View details',
      closeDetailBtn: 'Close details',
      recordSubDetail: 'Detail',
      recordSubSent: 'Prompt',
      recordSubResponse: 'Response',
      execFailed: (err) => `Execution failed: ${err}`,
      allowAndRerunBtn: 'Allow and Rerun',
      parseFailed: (err) => `Failed to parse findings from response: ${err} (Raw response available in "Response" tab)`,
      noFindings: 'No findings.',
      expandAllBtn: 'Expand all',
      collapseAllBtn: 'Collapse all',
      fixAllSevBtn: (sev, n) => `Mark all ${sev} as Fix (${n})`,
      dismissAllSevBtn: (sev, n) => `Mark all ${sev} as Ignore (${n})`,
      quoteVerified: 'Quote (verified in diff)',
      quoteNotVerified: 'Quote (not found in this diff)',
      noQuote: '(no quote)',
      memoPlaceholder: 'Note (what you fixed or why ignored — included in reports)',
      markDoneBtn: 'Mark as done',
      doneRevertBtn: '✓ Done (click to undo)',
      triageNoTimestamp: 'Triaged at: Not recorded (created before this feature)',
      triageCarrySource: 'carried from previous review',
      triageUserSource: 'your action',
      triageLastStamp: (whenStr, who, head7) => `Last triaged: ${whenStr} (${who}${head7 ? ` · HEAD ${head7}` : ''})`,
      idLabel: 'ID',
      copyBtn: 'Copy',
      copiedBtn: 'Copied',
      copyFailedBtn: 'Copy failed',
      sendToCliBtn: 'Send to CLI',
      writingCliBtn: 'Writing…',
      sendToCliTitle: 'Write this finding to a file and copy the 1-line prompt for your CLI',
      sendAllToCliBtn: (count) => `Send ${count} "Fix" items to CLI`,
      viewAllHandoffSummary: (count) => `View handoff text (${count} "Fix" items)`,
      copyHandoffTextBtn: 'Copy this text',
      mcpHint: '💡 From agy or Claude Code, you can handle these directly over MCP (mrcr mcp)',
      fetchAgentBtn: 'Import agent reports',
      fetchingAgentBtn: 'Importing…',
      fetchAgentTitle: 'Import the final status report written by your coding agent from conversation logs (reports saved with `mrcr handoff report` are already here)',
      agentStateAll: (reported, total) => `Agent reports: saved (${reported}/${total})`,
      agentStatePartial: (reported, total, missing) => `Agent reports: ${reported}/${total} saved · ${missing} missing`,
      agentStateNone: (missing) => `Agent reports: ${missing} missing`,
      agentStateDecide: (skipped) => `${skipped} declined by the agent (read the reason, then triage it or mark it done)`,
    },
  },
};

export function settingMetaUnit(m: SettingMeta, lang: Lang): string {
  return MESSAGES[lang].settingMeta[m.key]?.unit ?? m.unit;
}
