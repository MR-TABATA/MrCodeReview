//! 多言語（i18n）の定義と文言テーブル。
//!
//! 新しい言語（独 `de`・仏 `fr`・西 `es`・葡 `pt` など）を足すときは、
//! `Lang` 列挙型と `SUPPORTED`・`texts(lang)` のテーブルに 1 つずつ定義を足せばよく、
//! 呼び出し側（`lib.rs`・`recipe.rs`・`snapshot.rs`・`report.rs`・`report_xlsx.rs`・`settings.rs`）に
//! `if is_en` のような分岐を足す必要はない。

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    Ja,
    En,
}

pub const SUPPORTED: &[(&str, Lang)] = &[("ja", Lang::Ja), ("en", Lang::En)];

impl Lang {
    pub const fn code(self) -> &'static str {
        match self {
            Lang::Ja => "ja",
            Lang::En => "en",
        }
    }

    pub fn from_code(code: &str) -> Option<Lang> {
        SUPPORTED.iter().find(|(c, _)| *c == code).map(|(_, l)| *l)
    }

    /// OS のロケール文字列（`ja-JP`・`en_US.UTF-8` など）の先頭一致から解決する。
    pub fn from_locale_prefix(locale: &str) -> Option<Lang> {
        let lower = locale.to_ascii_lowercase();
        SUPPORTED
            .iter()
            .find(|(c, _)| lower == *c || lower.starts_with(&format!("{c}-")) || lower.starts_with(&format!("{c}_")) || lower.starts_with(&format!("{c}.")))
            .map(|(_, l)| *l)
    }

    pub fn is_valid_setting(val: &str) -> bool {
        val == "auto" || Self::from_code(val).is_some()
    }

    pub fn valid_setting_list() -> String {
        let mut codes = vec!["\"auto\"".to_string()];
        codes.extend(SUPPORTED.iter().map(|(c, _)| format!("\"{c}\"")));
        codes.join("、")
    }

    pub fn valid_arg_list() -> String {
        let mut codes = vec!["auto"];
        codes.extend(SUPPORTED.iter().map(|(c, _)| *c));
        codes.join(", ")
    }

    pub fn resolve_arg(arg: Option<&str>, sys_lang: Lang) -> Result<Lang, String> {
        match arg {
            None | Some("auto") => Ok(sys_lang),
            Some(c) => Self::from_code(c).ok_or_else(|| format!("不正な言語指定: {c}（{} のいずれか）", Self::valid_arg_list())),
        }
    }

    pub const fn texts(self) -> &'static Texts {
        match self {
            Lang::Ja => &TEXTS_JA,
            Lang::En => &TEXTS_EN,
        }
    }
}

/// 組み込み観点のテーブル（言語ごと）。
const BUILTIN_JA: &[(&str, &str)] = &[
    ("bugs", include_str!("../../../recipes/bugs.md")),
    ("errors", include_str!("../../../recipes/errors.md")),
    ("security", include_str!("../../../recipes/security.md")),
    ("performance", include_str!("../../../recipes/performance.md")),
    ("tests", include_str!("../../../recipes/tests.md")),
    ("consistency", include_str!("../../../recipes/consistency.md")),
    ("tf-state", include_str!("../../../recipes/tf-state.md")),
    ("tf-replace", include_str!("../../../recipes/tf-replace.md")),
    ("tf-repro", include_str!("../../../recipes/tf-repro.md")),
    ("tf-security", include_str!("../../../recipes/tf-security.md")),
    ("tf-refs", include_str!("../../../recipes/tf-refs.md")),
    ("doc-claims", include_str!("../../../recipes/doc-claims.md")),
    ("doc-reality", include_str!("../../../recipes/doc-reality.md")),
    ("doc-steps", include_str!("../../../recipes/doc-steps.md")),
    ("spec", include_str!("../../../recipes/spec.md")),
];

const BUILTIN_EN: &[(&str, &str)] = &[
    ("bugs", include_str!("../../../recipes/en/bugs.md")),
    ("errors", include_str!("../../../recipes/en/errors.md")),
    ("security", include_str!("../../../recipes/en/security.md")),
    ("performance", include_str!("../../../recipes/en/performance.md")),
    ("tests", include_str!("../../../recipes/en/tests.md")),
    ("consistency", include_str!("../../../recipes/en/consistency.md")),
    ("tf-state", include_str!("../../../recipes/en/tf-state.md")),
    ("tf-replace", include_str!("../../../recipes/en/tf-replace.md")),
    ("tf-repro", include_str!("../../../recipes/en/tf-repro.md")),
    ("tf-security", include_str!("../../../recipes/en/tf-security.md")),
    ("tf-refs", include_str!("../../../recipes/en/tf-refs.md")),
    ("doc-claims", include_str!("../../../recipes/en/doc-claims.md")),
    ("doc-reality", include_str!("../../../recipes/en/doc-reality.md")),
    ("doc-steps", include_str!("../../../recipes/en/doc-steps.md")),
    ("spec", include_str!("../../../recipes/en/spec.md")),
];

/// macOS が自動挿入するメニュー項目の既知タイトルと、言語ごとの翻訳先。
const MACOS_SYS_MENU_JA: &[(&str, &str)] = &[
    ("Writing Tools", "作文ツール"),
    ("AutoFill", "自動入力"),
    ("Start Dictation…", "音声入力を開始…"),
    ("Start Dictation...", "音声入力を開始…"),
    ("Emoji & Symbols", "絵文字と記号"),
    ("Spelling and Grammar", "スペルと文法"),
    ("Substitutions", "自動置換"),
    ("Transformations", "変換"),
    ("Speech", "スピーチ"),
];

const MACOS_SYS_MENU_EN: &[(&str, &str)] = &[
    ("作文ツール", "Writing Tools"),
    ("自動入力", "AutoFill"),
    ("音声入力を開始…", "Start Dictation…"),
    ("音声入力を開始...", "Start Dictation…"),
    ("絵文字と記号", "Emoji & Symbols"),
    ("スペルと文法", "Spelling and Grammar"),
    ("自動置換", "Substitutions"),
    ("変換", "Transformations"),
    ("スピーチ", "Speech"),
];

/// 組み込み観点のあらゆる言語でのタイトルから、標準 ID を逆引きする表。
/// 過去の履歴にどの言語でタイトルが記録されていても、現在の出力言語に変換できるようにする。
const BUILTIN_TITLE_ALIASES: &[(&str, &str)] = &[
    ("潜在バグ", "bugs"),
    ("潜在的な不具合", "bugs"),
    ("Latent Bugs", "bugs"),
    ("例外処理", "errors"),
    ("Error Handling", "errors"),
    ("セキュリティ", "security"),
    ("Security", "security"),
    ("パフォーマンス", "performance"),
    ("Performance", "performance"),
    ("テストの抜け", "tests"),
    ("Missing Tests", "tests"),
    ("他ファイルとの不整合", "consistency"),
    ("Cross-file Inconsistencies", "consistency"),
    ("状態・アドレス", "tf-state"),
    ("State & Addresses", "tf-state"),
    ("置換・破壊的変更", "tf-replace"),
    ("Replacement & Destruction", "tf-replace"),
    ("再現性・バージョン", "tf-repro"),
    ("Reproducibility & Versions", "tf-repro"),
    ("権限・公開・秘密", "tf-security"),
    ("Access, Exposure & Secrets", "tf-security"),
    ("参照の整合", "tf-refs"),
    ("Reference Consistency", "tf-refs"),
    ("根拠のない主張・数字", "doc-claims"),
    ("Unsupported Claims & Numbers", "doc-claims"),
    ("実在性・最新性の要確認", "doc-reality"),
    ("Reality & Currency Check", "doc-reality"),
    ("手順・前提・決定の抜け", "doc-steps"),
    ("Gaps in Steps, Assumptions & Decisions", "doc-steps"),
    ("設計書・仕様書との一致", "spec"),
    ("基準の文書との一致", "spec"),
    ("Specification Compliance", "spec"),
    ("リンター", "lint"),
    ("Linters", "lint"),
];

/// 比較元の定型ラベルから標準キーを逆引きする表。
const BASE_LABEL_ALIASES: &[(&str, &str)] = &[
    ("未コミットの変更", "HEAD"),
    ("Uncommitted changes", "HEAD"),
    ("直前のコミット", "HEAD~1"),
    ("直近のコミット", "HEAD~1"),
    ("Latest commit", "HEAD~1"),
    ("最初から（リポジトリ全体）", ":root"),
    ("Entire repository (from start)", ":root"),
];

/// CLI（`mrcr`）のレポートと進捗表示の文言。`{n}` などはプレースホルダ（`fill` で埋める）。
pub struct CliTexts {
    pub report_title: &'static str,
    pub report_repo: &'static str,
    pub report_base: &'static str,
    pub report_changed_files: &'static str,
    pub report_files: &'static str,
    pub report_timestamp: &'static str,
    pub report_summary: &'static str,
    pub report_no_findings: &'static str,
    pub report_total: &'static str,
    pub report_recipe_ok: &'static str,
    pub report_verified: &'static str,
    pub report_unverified: &'static str,
    pub report_location: &'static str,
    pub report_quote: &'static str,
    pub report_detail: &'static str,
    pub recipes_heading: &'static str,
    pub recipes_col_title: &'static str,
    pub recipes_col_source: &'static str,
    pub recipes_src_builtin: &'static str,
    pub recipes_src_repo: &'static str,
    pub review_start: &'static str,
    pub review_running: &'static str,
    pub review_done: &'static str,
    pub review_total: &'static str,
    pub review_history: &'static str,
}

/// `{name}` 形式のプレースホルダを順に埋める。
pub fn fill(tpl: &str, vars: &[(&str, &str)]) -> String {
    let mut out = tpl.to_string();
    for (k, v) in vars {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

pub struct Texts {
    pub lang: Lang,
    // ── メニュー ──
    pub menu_settings: &'static str,
    pub menu_update: &'static str,
    pub menu_services: &'static str,
    pub menu_hide_others: &'static str,
    pub menu_show_all: &'static str,
    pub menu_edit: &'static str,
    pub menu_undo: &'static str,
    pub menu_redo: &'static str,
    pub menu_cut: &'static str,
    pub menu_copy: &'static str,
    pub menu_paste: &'static str,
    pub menu_select_all: &'static str,
    pub menu_view: &'static str,
    pub menu_fullscreen: &'static str,
    pub menu_window: &'static str,
    pub menu_minimize: &'static str,
    pub menu_zoom: &'static str,
    pub menu_close_window: &'static str,
    pub macos_sys_menu_map: &'static [(&'static str, &'static str)],

    // ── 組み込み観点・プロンプト ──
    pub builtin_recipes: &'static [(&'static str, &'static str)],
    pub output_format: &'static str,
    pub diff_heading: &'static str,
    pub recipe_title_bugs: &'static str,
    pub recipe_title_errors: &'static str,
    pub recipe_title_security: &'static str,
    pub recipe_title_performance: &'static str,
    pub recipe_title_tests: &'static str,
    pub recipe_title_consistency: &'static str,
    pub recipe_title_tf_state: &'static str,
    pub recipe_title_tf_replace: &'static str,
    pub recipe_title_tf_repro: &'static str,
    pub recipe_title_tf_security: &'static str,
    pub recipe_title_tf_refs: &'static str,
    pub recipe_title_doc_claims: &'static str,
    pub recipe_title_doc_reality: &'static str,
    pub recipe_title_doc_steps: &'static str,
    pub recipe_title_spec: &'static str,
    pub recipe_title_lint: &'static str,

    // ── 比較元（snapshot） ──
    pub base_uncommitted: &'static str,
    pub base_uncommitted_hint: &'static str,
    pub base_latest: &'static str,
    pub base_previous_alias: &'static str,
    pub base_root: &'static str,
    pub base_root_hint: &'static str,
    pub base_commit_hint: &'static str,

    // ── レポート・Excel 共通ラベル ──
    pub list_sep: &'static str,
    pub state_fix: &'static str,
    pub state_wait: &'static str,
    pub state_dismiss: &'static str,
    pub state_new: &'static str,
    pub state_reverted_new: &'static str,
    pub state_reverted_new_lower: &'static str,
    pub fate_persists: &'static str,
    pub fate_gone: &'static str,
    pub fate_not_reraised: &'static str,
    pub fate_file_missing: &'static str,
    pub fate_unknown: &'static str,
    pub result_failed: &'static str,
    pub result_parse_err: &'static str,
    pub result_ok: &'static str,
    pub no_record: &'static str,
    pub unreported_lower: &'static str,
    pub unreported_cap: &'static str,
    pub tokens_failed_exec: &'static str,
    pub tokens_unreported_line: &'static str,
    pub all_repos: &'static str,
    pub period_selected: &'static str,
    pub period_all: &'static str,
    pub sum_repo: &'static str,
    pub sum_period: &'static str,
    pub sum_exported_at: &'static str,
    pub sum_exported_by: &'static str,
    pub sum_reviews: &'static str,
    pub sum_findings: &'static str,
    pub sum_notice: &'static str,
    pub sum_tokens: &'static str,
    pub report_title: &'static str,
    pub md_item_header: &'static str,
    pub no_reviews_in_period: &'static str,
    pub list_section_title: &'static str,
    pub md_list_header: &'static str,
    pub md_list_tokens_col: &'static str,
    pub html_list_header: &'static str,
    pub label_execution: &'static str,
    pub label_duration: &'static str,
    pub label_result: &'static str,
    pub label_failure_reason: &'static str,
    pub label_parse_err_reason: &'static str,
    pub label_range: &'static str,
    pub base_changes_suffix: &'static str,
    pub label_record: &'static str,
    pub label_run_note: &'static str,
    pub section_changes_from_prev: &'static str,
    pub no_title: &'static str,
    pub no_findings_detected: &'static str,
    pub unknown_place: &'static str,
    pub label_location: &'static str,
    pub label_evidence: &'static str,
    pub evidence_match_full: &'static str,
    pub evidence_mismatch_full: &'static str,
    pub label_detail: &'static str,
    pub label_triage_status: &'static str,
    pub done_suffix: &'static str,
    pub label_timeline: &'static str,
    pub label_last_triaged_at: &'static str,
    pub label_triage_memo: &'static str,
    pub label_carried_over: &'static str,
    pub label_subsequent_outcome: &'static str,
    pub label_agent_statement: &'static str,
    pub label_identifier: &'static str,
    pub ev_found_short: &'static str,
    pub ev_not_found_short: &'static str,
    pub ev_not_in_diff_xlsx: &'static str,
    pub no_snippet: &'static str,
    pub timeline_memo_added: &'static str,
    pub timeline_no_record: &'static str,

    // ── Excel シート名・列見出し ──
    pub xlsx_sheet_summary: &'static str,
    pub xlsx_summary_headers: &'static [(&'static str, f64)],
    pub xlsx_sheet_reviews: &'static str,
    pub xlsx_review_headers: &'static [(&'static str, f64)],
    pub xlsx_review_token_headers: &'static [(&'static str, f64)],
    pub xlsx_review_notes_header: (&'static str, f64),
    pub xlsx_sheet_findings: &'static str,
    pub xlsx_finding_headers_head: &'static [(&'static str, f64)],
    pub xlsx_finding_quotation_header: (&'static str, f64),
    pub xlsx_finding_headers_tail: &'static [(&'static str, f64)],
    pub xlsx_done_mark: &'static str,
    pub xlsx_source_carry: &'static str,
    pub xlsx_source_manual: &'static str,
    pub xlsx_sheet_history: &'static str,
    pub xlsx_history_headers: &'static [(&'static str, f64)],
    pub cli: CliTexts,
}

impl Texts {
    pub fn menu_about(&self, app_name: &str) -> String {
        match self.lang {
            Lang::Ja => format!("{app_name} について"),
            Lang::En => format!("About {app_name}"),
        }
    }

    pub fn menu_hide(&self, app_name: &str) -> String {
        match self.lang {
            Lang::Ja => format!("{app_name} を隠す"),
            Lang::En => format!("Hide {app_name}"),
        }
    }

    pub fn menu_quit(&self, app_name: &str) -> String {
        match self.lang {
            Lang::Ja => format!("{app_name} を終了"),
            Lang::En => format!("Quit {app_name}"),
        }
    }

    pub fn locked_notice(&self, ids: &[String]) -> String {
        let joined = ids.join(self.list_sep);
        match self.lang {
            Lang::Ja => format!(
                "このリポジトリには、自分の観点が {} 件あります（{joined}）。この版では、新しい観点（新しいボタン）は使えません。組み込みの観点と同じ名前のファイルで、文面を直すことはできます。",
                ids.len()
            ),
            Lang::En => format!(
                "This repository has {} custom review angle(s) ({joined}). Adding new review angles (new buttons) requires Pro. You can customize the wording of built-in angles by adding files with the same name.",
                ids.len()
            ),
        }
    }

    pub fn base_latest_hint(&self, short: &str, subject: &str) -> String {
        match self.lang {
            Lang::Ja => format!("{short} {subject}（最後のコミットと、コミットしていない変更）"),
            Lang::En => format!("{short} {subject} (Last commit and uncommitted changes)"),
        }
    }

    pub fn base_fork_label(&self, name: &str) -> String {
        match self.lang {
            Lang::Ja => format!("{name} との分岐点から"),
            Lang::En => format!("Fork point with {name}"),
        }
    }

    pub fn base_fork_hint(&self, what: &str, name: &str) -> String {
        match self.lang {
            Lang::Ja => format!("{what}（{name} から分かれたあとの変更すべて）"),
            Lang::En => format!("{what} (All changes since branching from {name})"),
        }
    }

    pub fn base_from_commit(&self, short_hash: &str, subject: &str) -> String {
        match self.lang {
            Lang::Ja => format!("{short_hash} {subject} から"),
            Lang::En => format!("From {short_hash} {subject}"),
        }
    }

    pub fn fmt_secs(&self, ms: u64) -> String {
        match self.lang {
            Lang::Ja => {
                if ms < 1000 {
                    format!("{ms} ミリ秒")
                } else {
                    format!("{:.1} 秒", ms as f64 / 1000.0)
                }
            }
            Lang::En => {
                if ms < 1000 {
                    format!("{ms} ms")
                } else {
                    format!("{:.1} s", ms as f64 / 1000.0)
                }
            }
        }
    }

    pub fn state_label(&self, s: &str) -> &'static str {
        match s {
            "fix" => self.state_fix,
            "wait" => self.state_wait,
            "dismiss" => self.state_dismiss,
            _ => self.state_new,
        }
    }

    pub fn fate_label(&self, f: &str) -> &'static str {
        match f {
            "persists" => self.fate_persists,
            "gone" => self.fate_gone,
            "not_reraised" => self.fate_not_reraised,
            "file_missing" => self.fate_file_missing,
            _ => self.fate_unknown,
        }
    }

    pub fn recipe_title<'a>(&self, title: &'a str) -> &'a str {
        let Some((_, id)) = BUILTIN_TITLE_ALIASES.iter().find(|(alias, _)| *alias == title) else {
            return title;
        };
        match *id {
            "bugs" => self.recipe_title_bugs,
            "errors" => self.recipe_title_errors,
            "security" => self.recipe_title_security,
            "performance" => self.recipe_title_performance,
            "tests" => self.recipe_title_tests,
            "consistency" => self.recipe_title_consistency,
            "tf-state" => self.recipe_title_tf_state,
            "tf-replace" => self.recipe_title_tf_replace,
            "tf-repro" => self.recipe_title_tf_repro,
            "tf-security" => self.recipe_title_tf_security,
            "tf-refs" => self.recipe_title_tf_refs,
            "doc-claims" => self.recipe_title_doc_claims,
            "doc-reality" => self.recipe_title_doc_reality,
            "doc-steps" => self.recipe_title_doc_steps,
            "spec" => self.recipe_title_spec,
            "lint" => self.recipe_title_lint,
            _ => title,
        }
    }

    pub fn base_label<'a>(&self, label: &'a str) -> &'a str {
        let Some((_, key)) = BASE_LABEL_ALIASES.iter().find(|(alias, _)| *alias == label) else {
            return label;
        };
        match *key {
            "HEAD" => self.base_uncommitted,
            "HEAD~1" => self.base_previous_alias,
            ":root" => self.base_root,
            _ => label,
        }
    }

    pub fn timeline_event(&self, dt: &str, state_str: &str, is_carry: bool, head_short: Option<&str>) -> String {
        match self.lang {
            Lang::Ja => {
                let how = if is_carry { "前回のレビューから引き継ぎ" } else { "あなたの操作" };
                let head = head_short.map(|h| format!("・HEAD {h}")).unwrap_or_default();
                format!("{dt} — {state_str}（{how}{head}）")
            }
            Lang::En => {
                let how = if is_carry { "carried from previous review" } else { "manual action" };
                let head = head_short.map(|h| format!(", HEAD {h}")).unwrap_or_default();
                format!("{dt} — {state_str} ({how}{head})")
            }
        }
    }

    pub fn timeline_done(&self, dt: &str, head_short: Option<&str>) -> String {
        match self.lang {
            Lang::Ja => {
                let head = head_short.map(|h| format!("・HEAD {h}")).unwrap_or_default();
                format!("{dt} — 対応済みにした（あなたの操作{head}）")
            }
            Lang::En => {
                let head = head_short.map(|h| format!(", HEAD {h}")).unwrap_or_default();
                format!("{dt} — Marked as Done (manual action{head})")
            }
        }
    }

    pub fn agent_text(&self, is_did: bool, dt: &str, text: &str) -> String {
        match self.lang {
            Lang::Ja => {
                let label = if is_did { "対応した" } else { "対応しなかった" };
                format!("{label}（エージェントの申告・{dt}。実際に直ったかの確認ではありません）: {text}")
            }
            Lang::En => {
                let label = if is_did { "Resolved" } else { "Not resolved" };
                format!("{label} (agent statement at {dt}. Not verified as actual fix): {text}")
            }
        }
    }

    pub fn later_text(&self, dt: &str, fate_label: &str) -> String {
        match self.lang {
            Lang::Ja => format!("{dt} のレビューで「{fate_label}」"),
            Lang::En => format!("\"{fate_label}\" in review at {dt}"),
        }
    }

    pub fn period_selected_range(&self, first: &str, last: &str) -> String {
        match self.lang {
            Lang::Ja => format!("選んだレビュー（{first} 〜 {last}）"),
            Lang::En => format!("Selected reviews ({first} to {last})"),
        }
    }

    pub fn period_since(&self, since: &str) -> String {
        match self.lang {
            Lang::Ja => format!("{since} 以降"),
            Lang::En => format!("Since {since}"),
        }
    }

    pub fn period_until(&self, until: &str) -> String {
        match self.lang {
            Lang::Ja => format!("{until} まで"),
            Lang::En => format!("Until {until}"),
        }
    }

    pub fn period_between(&self, since: &str, until: &str) -> String {
        match self.lang {
            Lang::Ja => format!("{since} 〜 {until}"),
            Lang::En => format!("{since} to {until}"),
        }
    }

    pub fn tokens_pair(&self, input: &str, output: &str) -> String {
        match self.lang {
            Lang::Ja => format!("入力 {input}・出力 {output}"),
            Lang::En => format!("Input {input} · Output {output}"),
        }
    }

    pub fn tokens_reported_line(&self, pair: &str) -> String {
        match self.lang {
            Lang::Ja => format!("{pair}（提供元が報告した値）"),
            Lang::En => format!("{pair} (reported by provider)"),
        }
    }

    pub fn exported_at_val(&self, dt: &str, offset: &str) -> String {
        match self.lang {
            Lang::Ja => format!("{dt}（時差 {offset}）"),
            Lang::En => format!("{dt} (UTC{offset})"),
        }
    }

    pub fn reviews_count_val(&self, reviews: usize, failed: usize) -> String {
        match self.lang {
            Lang::Ja => format!("{reviews} 件{}", if failed > 0 { format!("（失敗 {failed} 件）") } else { String::new() }),
            Lang::En => format!("{reviews} review(s){}", if failed > 0 { format!(" ({failed} failed)") } else { String::new() }),
        }
    }

    pub fn findings_count_val(&self, findings: usize, fix: usize, done: usize, wait: usize, dismiss: usize, new: usize) -> String {
        match self.lang {
            Lang::Ja => format!("{findings} 件 — 対応 {fix}（うち対応済み {done}）・待ち {wait}・無視 {dismiss}・未仕分け {new}"),
            Lang::En => format!("{findings} finding(s) — Fix: {fix} (Done: {done}), Wait: {wait}, Dismiss: {dismiss}, New: {new}"),
        }
    }

    pub fn missing_ids_notice(&self, count: usize, ids_joined: &str) -> String {
        match self.lang {
            Lang::Ja => format!("選んだレビューのうち {count} 件は、履歴に見つかりませんでした（削除された可能性があります）: {ids_joined}"),
            Lang::En => format!("{count} selected review(s) were not found in history (may have been deleted): {ids_joined}"),
        }
    }

    pub fn unreadable_notice(&self, unreadable: usize) -> String {
        match self.lang {
            Lang::Ja => format!("壊れていて読めなかった記録が {unreadable} 件あり、この出力に含まれていません（データベースが壊れている可能性があります）"),
            Lang::En => format!("{unreadable} corrupted record(s) could not be read and are excluded from this export (database may be damaged)"),
        }
    }

    pub fn summary_tokens_val(&self, input: &str, output: &str, cache_read: Option<&str>, cache_write: Option<&str>, unreported: usize) -> String {
        match self.lang {
            Lang::Ja => {
                let mut s = format!("入力 {input}・出力 {output}");
                if let Some(cr) = cache_read {
                    s.push_str(&format!("・キャッシュ読み取り {cr}"));
                }
                if let Some(cw) = cache_write {
                    s.push_str(&format!("・キャッシュ書き込み {cw}"));
                }
                s.push_str("（提供元が報告した値だけ");
                if unreported > 0 {
                    s.push_str(&format!("。報告の無かったレビュー {unreported} 件は、含めていません"));
                }
                s.push('）');
                s
            }
            Lang::En => {
                let mut s = format!("Input {input} · Output {output}");
                if let Some(cr) = cache_read {
                    s.push_str(&format!(" · Cache Read {cr}"));
                }
                if let Some(cw) = cache_write {
                    s.push_str(&format!(" · Cache Write {cw}"));
                }
                s.push_str(" (reported values only");
                if unreported > 0 {
                    s.push_str(&format!("; {unreported} review(s) without token reports excluded"));
                }
                s.push(')');
                s
            }
        }
    }

    pub fn notice(&self, app_name: &str) -> String {
        match self.lang {
            Lang::Ja => format!("この記録は、{app_name} がレビューの実行時に記録した値と、あなたの操作（仕分け・対応メモ・対応済み）を、そのまま出力したものです。アプリが推測した内容には、「推定」と書いています。仕分けの時刻が残っていないものは、「記録なし」と書いています。"),
            Lang::En => format!("This record was generated from values recorded by {app_name} during review runs and your triage actions (classification, memos, and done marks). Items inferred by the app are marked as \"inferred\". Items without recorded triage timestamps are marked as \"no record\"."),
        }
    }

    pub fn record_tag_md(&self, head: &str, diff: &str, base: Option<&str>, tag: &str) -> String {
        match self.lang {
            Lang::Ja => format!(
                "HEAD {head} ・ 差分 {diff}{} ・ 識別子 {tag}",
                base.map(|b| format!(" ・ 比較元 {b}")).unwrap_or_default()
            ),
            Lang::En => format!(
                "HEAD {head} · Diff {diff}{} · Tag {tag}",
                base.map(|b| format!(" · Base {b}")).unwrap_or_default()
            ),
        }
    }

    pub fn record_tag_html(&self, head: &str, diff: &str, base: Option<&str>, tag: &str) -> String {
        match self.lang {
            Lang::Ja => format!(
                "HEAD <code>{head}</code> ・ 差分 <code>{diff}</code>{} ・ 識別子 <code>{tag}</code>",
                base.map(|b| format!(" ・ 比較元 <code>{b}</code>")).unwrap_or_default()
            ),
            Lang::En => format!(
                "HEAD <code>{head}</code> · Diff <code>{diff}</code>{} · Tag <code>{tag}</code>",
                base.map(|b| format!(" · Base <code>{b}</code>")).unwrap_or_default()
            ),
        }
    }

    pub fn files_sent_label(&self, count: usize) -> String {
        match self.lang {
            Lang::Ja => format!("送ったファイル（{count} 件）"),
            Lang::En => format!("Files sent ({count} file(s))"),
        }
    }

    pub fn files_excluded_label(&self, count: usize) -> String {
        match self.lang {
            Lang::Ja => format!("送らなかったもの（{count} 件）"),
            Lang::En => format!("Files excluded ({count} file(s))"),
        }
    }

    pub fn prev_state_label(&self, state_label: &str) -> String {
        match self.lang {
            Lang::Ja => format!("前回は「{state_label}」"),
            Lang::En => format!("previous: \"{state_label}\""),
        }
    }

    pub fn findings_heading(&self, count: usize) -> String {
        match self.lang {
            Lang::Ja => format!("指摘（{count} 件）"),
            Lang::En => format!("Findings ({count} finding(s))"),
        }
    }

    pub fn rule_inline_md(&self, code: &str) -> String {
        match self.lang {
            Lang::Ja => format!(" ・ 規則 {code}"),
            Lang::En => format!(" · Rule {code}"),
        }
    }

    pub fn rule_inline_html(&self, esc_rule: &str) -> String {
        match self.lang {
            Lang::Ja => format!(" ・ 規則 <code>{esc_rule}</code>"),
            Lang::En => format!(" · Rule <code>{esc_rule}</code>"),
        }
    }

    pub fn carried_over_md(&self, code_tag: &str) -> String {
        match self.lang {
            Lang::Ja => format!("前回のレビュー（{code_tag}）から、仕分けを引き継いだ"),
            Lang::En => format!("Carried over from previous review ({code_tag})"),
        }
    }

    pub fn carried_over_html(&self, esc_tag: &str) -> String {
        match self.lang {
            Lang::Ja => format!("前回のレビュー（<code>{esc_tag}</code>）から、仕分けを引き継いだ"),
            Lang::En => format!("Carried over from previous review (<code>{esc_tag}</code>)"),
        }
    }

    pub fn quotation_label_md(&self, status: &str) -> String {
        match self.lang {
            Lang::Ja => format!("引用（{status}）"),
            Lang::En => format!("Quotation ({status})"),
        }
    }

    pub fn quotation_label_html(&self, status: &str) -> String {
        match self.lang {
            Lang::Ja => format!("引用（{status}）"),
            Lang::En => format!("Quoted code ({status})"),
        }
    }

    pub fn prompt_sent_label(&self, chars: usize) -> String {
        match self.lang {
            Lang::Ja => format!("送った文章の全文（{chars} 文字）"),
            Lang::En => format!("Prompt sent to LLM ({chars} characters)"),
        }
    }

    pub fn xlsx_failed_note(&self, err: &str) -> String {
        match self.lang {
            Lang::Ja => format!("失敗: {err}"),
            Lang::En => format!("Failed: {err}"),
        }
    }

    pub fn xlsx_parse_err_note(&self, err: &str) -> String {
        match self.lang {
            Lang::Ja => format!("指摘を読めなかった: {err}"),
            Lang::En => format!("Parse error: {err}"),
        }
    }
}

pub const OUTPUT_FORMAT_JA: &str = r#"## 出力形式

次の JSON だけを返してください。前後に説明文やコードフェンスを付けないでください。

{"findings":[{"file":"差分に出ているファイルのパス","line":新しい側の行番号（不明なら null）,"severity":"high|medium|low","title":"件名","detail":"本文","evidence":"差分からそのまま引用した 1 行","spec_quote":"基準の文書からそのまま引用した 1 行またはフレーズ（基準文書を参照する観点のみ）"}]}

`title`（件名）と `detail`（本文）は、平易な日本語で、読む人が「つまり何？」と迷わないように書いてください。
- `title`: メールの件名のように、読むだけで中身が分かる 1 文にする。「何をすると・どうなる」を言い切る。30 字前後。
  - 良い例: 「空の配列を渡すと 0 で割ってしまい、落ちる」
  - 悪い例: 「境界値の扱い」「エラー処理の不備」（分類名だけで、中身が分からない）
- `detail`: 「どんなとき（入力や状況）」「どうなる（誤った結果）」「どう直す」の 3 点を、短い文で書く。1 文ごとに改行する（JSON の文字列の中では `\n`）。1 文は 40 字前後、全体で 3〜4 文まで。専門用語は、できるだけ使わない。
  - 変数名・関数名・型名・列名・内部の用語（`i64`、`findings_count` など）は、`title` に書かない。`detail` に書くときも、短い言い換えを添える。
  - 「ユーザーの画面や結果で、何が起きるか」を、必ず 1 文入れる（例: 「左の件数が、20 桁の数字になる」）。コードの中の話だけで終わらせない。
- 基準の文書を参照する観点（spec）のときは、基準の記述から一字一句違わずに抜き出した原文フレーズを `spec_quote` に必ず含めてください。

指摘が無ければ {"findings":[]} を返してください。"#;

pub const OUTPUT_FORMAT_EN: &str = r#"## Output Format

Return only the following JSON. Do not include any explanations or markdown code fences before or after.

{"findings":[{"file":"path of the file in the diff","line":line number on the new side (or null if unknown),"severity":"high|medium|low","title":"subject line","detail":"body","evidence":"an exact line quoted from the diff","spec_quote":"an exact line or phrase quoted from the specification (spec recipe only)"}]}

Write `title` and `detail` in plain language so the reader never has to ask "so what is this about?".
- `title`: Like an email subject, one sentence that tells the content on its own: what happens, under what action. About 12 words.
  - Good: "Passing an empty array divides by zero and crashes"
  - Bad: "Boundary handling", "Error handling issue" (a category name; the content is unclear)
- `detail`: Cover three points in short sentences: when (input or situation), what goes wrong (the wrong result), and how to fix it. Put each sentence on its own line (`\n` inside the JSON string). At most 3-4 sentences, about 20 words each. Avoid jargon where possible.
  - Never put variable, function, type or column names or internal terms (`i64`, `findings_count`, ...) in `title`. In `detail`, add a short plain-language gloss next to any you must use.
  - Always include one sentence on what the user sees or gets (e.g. "the count on the left becomes a 20-digit number"). Do not stop at what happens inside the code.
- When reviewing against a specification (spec recipe), include the exact quoted phrase from the specification in `spec_quote`.

If there are no findings, return {"findings":[]}."#;

pub static TEXTS_JA: Texts = Texts {
    lang: Lang::Ja,
    menu_settings: "設定…",
    menu_update: "アップデートを確認…",
    menu_services: "サービス",
    menu_hide_others: "ほかを隠す",
    menu_show_all: "すべてを表示",
    menu_edit: "編集",
    menu_undo: "取り消す",
    menu_redo: "やり直す",
    menu_cut: "切り取り",
    menu_copy: "コピー",
    menu_paste: "ペースト",
    menu_select_all: "すべてを選択",
    menu_view: "表示",
    menu_fullscreen: "フルスクリーンにする",
    menu_window: "ウインドウ",
    menu_minimize: "しまう",
    menu_zoom: "拡大/縮小",
    menu_close_window: "閉じる",
    macos_sys_menu_map: MACOS_SYS_MENU_JA,

    builtin_recipes: BUILTIN_JA,
    output_format: OUTPUT_FORMAT_JA,
    diff_heading: "## 差分",
    recipe_title_bugs: "潜在バグ",
    recipe_title_errors: "例外処理",
    recipe_title_security: "セキュリティ",
    recipe_title_performance: "パフォーマンス",
    recipe_title_tests: "テストの抜け",
    recipe_title_consistency: "他ファイルとの不整合",
    recipe_title_tf_state: "状態・アドレス",
    recipe_title_tf_replace: "置換・破壊的変更",
    recipe_title_tf_repro: "再現性・バージョン",
    recipe_title_tf_security: "権限・公開・秘密",
    recipe_title_tf_refs: "参照の整合",
    recipe_title_doc_claims: "根拠のない主張・数字",
    recipe_title_doc_reality: "実在性・最新性の要確認",
    recipe_title_doc_steps: "手順・前提・決定の抜け",
    recipe_title_spec: "設計書・仕様書との一致",
    recipe_title_lint: "リンター",

    base_uncommitted: "未コミットの変更",
    base_uncommitted_hint: "コミットしていない変更だけ",
    base_latest: "直近のコミット",
    base_previous_alias: "直前のコミット",
    base_root: "最初から（リポジトリ全体）",
    base_root_hint: "最初のコミットの内容も含めて、何も無い状態から、いままでの全部。大きいと、送る量の上限で切れる",
    base_commit_hint: "このコミット以降すべて（このコミットを含む）",

    list_sep: "、",
    state_fix: "対応",
    state_wait: "待ち",
    state_dismiss: "無視",
    state_new: "未仕分け",
    state_reverted_new: "未仕分けに戻した",
    state_reverted_new_lower: "未仕分けに戻した",
    fate_persists: "継続（次のレビューでも指摘された）",
    fate_gone: "解消（引用した行が、ファイルから消えた。推定）",
    fate_not_reraised: "再指摘なし（次のレビューでは指摘されなかったが、引用した行は、まだある）",
    fate_file_missing: "ファイルが見つからない（削除・移動の可能性）",
    fate_unknown: "不明（引用が空などで、調べられなかった）",
    result_failed: "失敗",
    result_parse_err: "答えは返ったが、指摘を読めなかった",
    result_ok: "成功",
    no_record: "記録なし",
    unreported_lower: "報告なし",
    unreported_cap: "報告なし",
    tokens_failed_exec: "記録なし（失敗した実行）",
    tokens_unreported_line: "報告なし（提供元が、値を返しませんでした）",
    all_repos: "すべて",
    period_selected: "選んだレビュー",
    period_all: "すべて",
    sum_repo: "リポジトリ",
    sum_period: "期間",
    sum_exported_at: "出力日時",
    sum_exported_by: "出力したアプリ",
    sum_reviews: "レビュー",
    sum_findings: "指摘",
    sum_notice: "注意",
    sum_tokens: "トークン",
    report_title: "レビューの記録",
    md_item_header: "| 項目 | 内容 |\n| --- | --- |\n",
    no_reviews_in_period: "この期間に、記録されたレビューはありません。",
    list_section_title: "一覧",
    md_list_header: "| # | 日時 | 観点 | LLM | 比較元 | 結果 | 指摘 | 対応 | 待ち | 無視 | 未 | 済 |",
    md_list_tokens_col: " トークン |",
    html_list_header: "<h2>一覧</h2>\n<table class=\"rlist\"><thead><tr><th>#</th><th>日時</th><th>観点</th><th>LLM</th><th>比較元</th><th>結果</th><th>指摘</th><th>対応</th><th>待ち</th><th>無視</th><th>未</th><th>済</th>",
    label_execution: "実行",
    label_duration: "所要",
    label_result: "結果",
    label_failure_reason: "失敗の理由",
    label_parse_err_reason: "指摘を読めなかった理由",
    label_range: "比較元",
    base_changes_suffix: "（いまの作業ツリーまでの変更）",
    label_record: "記録",
    label_run_note: "実行メモ",
    section_changes_from_prev: "前回のレビューからの変化",
    no_title: "(見出しなし)",
    no_findings_detected: "指摘はありませんでした。",
    unknown_place: "(不明)",
    label_location: "場所",
    label_evidence: "根拠",
    evidence_match_full: "引用が、差分に実在する（文字列として一致。指摘が正しいことの確認ではない）",
    evidence_mismatch_full: "引用が、この差分に見つからない（根拠不一致）",
    label_detail: "理由",
    label_triage_status: "現在の扱い",
    done_suffix: "・対応済み",
    label_timeline: "経過",
    label_last_triaged_at: "最後に仕分けた時刻",
    label_triage_memo: "対応メモ",
    label_carried_over: "引き継ぎ",
    label_subsequent_outcome: "その後",
    label_agent_statement: "エージェントの申告",
    label_identifier: "識別子",
    ev_found_short: "差分に実在",
    ev_not_found_short: "差分に見つからない",
    ev_not_in_diff_xlsx: "根拠不一致",
    no_snippet: "(引用なし)",
    timeline_memo_added: "対応メモを書いた",
    timeline_no_record: "仕分けた時刻: 記録なし（この機能より前の記録）",

    xlsx_sheet_summary: "概要",
    xlsx_summary_headers: &[("項目", 22.0), ("内容", 100.0)],
    xlsx_sheet_reviews: "レビュー",
    xlsx_review_headers: &[
        ("#", 5.0),
        ("日時", 17.0),
        ("観点", 22.0),
        ("LLM", 24.0),
        ("モデル", 20.0),
        ("比較元", 30.0),
        ("結果", 18.0),
        ("所要時間（秒）", 12.0),
        ("指摘", 7.0),
        ("対応", 7.0),
        ("待ち", 7.0),
        ("無視", 7.0),
        ("未仕分け", 9.0),
        ("対応済み", 9.0),
        ("HEAD", 12.0),
        ("差分", 14.0),
        ("識別子", 34.0),
    ],
    xlsx_review_token_headers: &[("入力トークン", 13.0), ("出力トークン", 13.0), ("キャッシュ読み取り", 14.0), ("キャッシュ書き込み", 14.0)],
    xlsx_review_notes_header: ("実行メモ・失敗の理由", 60.0),
    xlsx_sheet_findings: "指摘",
    xlsx_finding_headers_head: &[
        ("レビュー#", 9.0),
        ("レビュー日時", 17.0),
        ("観点", 20.0),
        ("指摘#", 7.0),
        ("ファイル", 36.0),
        ("行", 7.0),
        ("重大度", 9.0),
        ("規則", 18.0),
        ("タイトル", 40.0),
        ("理由", 60.0),
    ],
    xlsx_finding_quotation_header: ("引用", 50.0),
    xlsx_finding_headers_tail: &[
        ("根拠", 12.0),
        ("現在の扱い", 11.0),
        ("対応済み", 10.0),
        ("最後に仕分けた日時", 17.0),
        ("仕分けの方法", 14.0),
        ("対応済みの日時", 17.0),
        ("対応済みの HEAD", 14.0),
        ("対応メモ", 50.0),
        ("その後（次のレビューでの変化）", 50.0),
        ("エージェントの申告（ログから。実際に直ったかの確認ではない）", 50.0),
        ("識別子", 38.0),
    ],
    xlsx_done_mark: "済",
    xlsx_source_carry: "前回から引き継ぎ",
    xlsx_source_manual: "あなたの操作",
    xlsx_sheet_history: "仕分けの履歴",
    xlsx_history_headers: &[
        ("レビュー#", 9.0),
        ("レビュー日時", 17.0),
        ("観点", 20.0),
        ("指摘#", 7.0),
        ("タイトル", 40.0),
        ("日時", 17.0),
        ("状態", 11.0),
        ("方法", 16.0),
        ("そのときの HEAD", 16.0),
    ],
    cli: CliTexts {
        report_title: "MrCodeReview レポート",
        report_repo: "リポジトリ",
        report_base: "比較範囲",
        report_changed_files: "変更ファイル数",
        report_files: "{n} ファイル",
        report_timestamp: "実施日時",
        report_summary: "総合サマリー",
        report_no_findings: "✓ **全観点で指摘はありませんでした。**",
        report_total: "**合計指摘数**: {total} 件（根拠一致: {verified} 件、根拠不一致: {unverified} 件）",
        report_recipe_ok: "指摘はありませんでした (OK)",
        report_verified: "根拠あり",
        report_unverified: "⚠️ 根拠不一致（ハルシネーションの可能性あり）",
        report_location: "場所",
        report_quote: "引用コード",
        report_detail: "詳細",
        recipes_heading: "利用可能なレビュー観点一覧:",
        recipes_col_title: "タイトル",
        recipes_col_source: "提供元",
        recipes_src_builtin: "組み込み",
        recipes_src_repo: "リポジトリ自作",
        review_start: "レビュー開始: {n} ファイルの変更 (比較元: {base})",
        review_running: "[{i}/{n}] レビュー中: {title} ({id}) ...",
        review_done: "✅ レビュー完了: {path}",
        review_total: "合計指摘数: {total} 件（根拠一致: {verified} 件、根拠不一致: {unverified} 件）",
        review_history: "履歴保存先: {path} (GUI アプリと共有)",
    },
};

pub static TEXTS_EN: Texts = Texts {
    lang: Lang::En,
    menu_settings: "Settings…",
    menu_update: "Check for Updates…",
    menu_services: "Services",
    menu_hide_others: "Hide Others",
    menu_show_all: "Show All",
    menu_edit: "Edit",
    menu_undo: "Undo",
    menu_redo: "Redo",
    menu_cut: "Cut",
    menu_copy: "Copy",
    menu_paste: "Paste",
    menu_select_all: "Select All",
    menu_view: "View",
    menu_fullscreen: "Toggle Full Screen",
    menu_window: "Window",
    menu_minimize: "Minimize",
    menu_zoom: "Zoom",
    menu_close_window: "Close Window",
    macos_sys_menu_map: MACOS_SYS_MENU_EN,

    builtin_recipes: BUILTIN_EN,
    output_format: OUTPUT_FORMAT_EN,
    diff_heading: "## Diff",
    recipe_title_bugs: "Latent Bugs",
    recipe_title_errors: "Error Handling",
    recipe_title_security: "Security",
    recipe_title_performance: "Performance",
    recipe_title_tests: "Missing Tests",
    recipe_title_consistency: "Cross-file Inconsistencies",
    recipe_title_tf_state: "State & Addresses",
    recipe_title_tf_replace: "Replacement & Destruction",
    recipe_title_tf_repro: "Reproducibility & Versions",
    recipe_title_tf_security: "Access, Exposure & Secrets",
    recipe_title_tf_refs: "Reference Consistency",
    recipe_title_doc_claims: "Unsupported Claims & Numbers",
    recipe_title_doc_reality: "Reality & Currency Check",
    recipe_title_doc_steps: "Gaps in Steps, Assumptions & Decisions",
    recipe_title_spec: "Specification Compliance",
    recipe_title_lint: "Linters",

    base_uncommitted: "Uncommitted changes",
    base_uncommitted_hint: "Only uncommitted changes",
    base_latest: "Latest commit",
    base_previous_alias: "Latest commit",
    base_root: "Entire repository (from start)",
    base_root_hint: "From the initial commit to current state. If large, limits may truncate.",
    base_commit_hint: "All changes from this commit forward (including this commit)",

    list_sep: ", ",
    state_fix: "Fix",
    state_wait: "Wait",
    state_dismiss: "Dismiss",
    state_new: "New",
    state_reverted_new: "Reverted to New",
    state_reverted_new_lower: "reverted to New",
    fate_persists: "Persisting (re-detected in subsequent review)",
    fate_gone: "Resolved (quoted line removed from file; inferred)",
    fate_not_reraised: "Not re-detected (quoted line remains in file)",
    fate_file_missing: "File missing (possibly deleted or moved)",
    fate_unknown: "Unknown (unable to check, e.g. empty evidence)",
    result_failed: "Failed",
    result_parse_err: "Responded, but failed to parse findings",
    result_ok: "Success",
    no_record: "No record",
    unreported_lower: "unreported",
    unreported_cap: "Unreported",
    tokens_failed_exec: "No record (failed execution)",
    tokens_unreported_line: "Unreported (provider returned no values)",
    all_repos: "All",
    period_selected: "Selected reviews",
    period_all: "All time",
    sum_repo: "Repository",
    sum_period: "Period",
    sum_exported_at: "Exported At",
    sum_exported_by: "Exported by",
    sum_reviews: "Reviews",
    sum_findings: "Findings",
    sum_notice: "Notice",
    sum_tokens: "Tokens",
    report_title: "Review Report",
    md_item_header: "| Item | Details |\n| --- | --- |\n",
    no_reviews_in_period: "No reviews were recorded in this period.",
    list_section_title: "Review List",
    md_list_header: "| # | Date/Time | Angle | LLM | Range | Result | Findings | Fix | Wait | Dismiss | New | Done |",
    md_list_tokens_col: " Tokens |",
    html_list_header: "<h2>Review List</h2>\n<table class=\"rlist\"><thead><tr><th>#</th><th>Date/Time</th><th>Angle</th><th>LLM</th><th>Range</th><th>Result</th><th>Findings</th><th>Fix</th><th>Wait</th><th>Dismiss</th><th>New</th><th>Done</th>",
    label_execution: "Execution",
    label_duration: "duration",
    label_result: "Result",
    label_failure_reason: "Failure reason",
    label_parse_err_reason: "Parse error reason",
    label_range: "Range",
    base_changes_suffix: " (changes up to current working tree)",
    label_record: "Record",
    label_run_note: "Run note",
    section_changes_from_prev: "Changes from previous review",
    no_title: "(No title)",
    no_findings_detected: "No findings detected.",
    unknown_place: "(Unknown)",
    label_location: "Location",
    label_evidence: "Evidence",
    evidence_match_full: "Quotation matches diff verbatim (exact text match; does not imply correctness)",
    evidence_mismatch_full: "Quotation not found in this diff (evidence mismatch)",
    label_detail: "Detail",
    label_triage_status: "Triage status",
    done_suffix: " · Done",
    label_timeline: "Timeline",
    label_last_triaged_at: "Last triaged at",
    label_triage_memo: "Triage memo",
    label_carried_over: "Carried over",
    label_subsequent_outcome: "Subsequent outcome",
    label_agent_statement: "Agent statement",
    label_identifier: "Identifier",
    ev_found_short: "Found in diff",
    ev_not_found_short: "Not in diff",
    ev_not_in_diff_xlsx: "Not in diff",
    no_snippet: "(No snippet)",
    timeline_memo_added: "Added/updated triage memo",
    timeline_no_record: "Triaged time: No record (prior to tracking)",

    xlsx_sheet_summary: "Summary",
    xlsx_summary_headers: &[("Item", 22.0), ("Details", 100.0)],
    xlsx_sheet_reviews: "Reviews",
    xlsx_review_headers: &[
        ("#", 5.0),
        ("Date/Time", 17.0),
        ("Angle", 22.0),
        ("LLM", 24.0),
        ("Model", 20.0),
        ("Range", 30.0),
        ("Result", 18.0),
        ("Duration (s)", 12.0),
        ("Findings", 8.0),
        ("Fix", 7.0),
        ("Wait", 7.0),
        ("Dismiss", 7.0),
        ("New", 7.0),
        ("Done", 7.0),
        ("HEAD", 12.0),
        ("Diff", 14.0),
        ("Tag", 34.0),
    ],
    xlsx_review_token_headers: &[("Input Tokens", 13.0), ("Output Tokens", 13.0), ("Cache Read", 14.0), ("Cache Write", 14.0)],
    xlsx_review_notes_header: ("Notes / Failure Reason", 60.0),
    xlsx_sheet_findings: "Findings",
    xlsx_finding_headers_head: &[
        ("Review #", 9.0),
        ("Review Date/Time", 17.0),
        ("Angle", 20.0),
        ("Finding #", 9.0),
        ("File", 36.0),
        ("Line", 7.0),
        ("Severity", 9.0),
        ("Rule", 18.0),
        ("Title", 40.0),
        ("Detail", 60.0),
    ],
    xlsx_finding_quotation_header: ("Quotation", 50.0),
    xlsx_finding_headers_tail: &[
        ("Evidence", 12.0),
        ("Triage Status", 11.0),
        ("Done", 8.0),
        ("Last Triaged", 17.0),
        ("Triage Method", 14.0),
        ("Done Date/Time", 17.0),
        ("Done HEAD", 14.0),
        ("Triage Memo", 50.0),
        ("Subsequent Outcome", 50.0),
        ("Agent Statement", 50.0),
        ("Tag", 38.0),
    ],
    xlsx_done_mark: "Done",
    xlsx_source_carry: "Carried over",
    xlsx_source_manual: "Manual action",
    xlsx_sheet_history: "Triage History",
    xlsx_history_headers: &[
        ("Review #", 9.0),
        ("Review Date/Time", 17.0),
        ("Angle", 20.0),
        ("Finding #", 9.0),
        ("Title", 40.0),
        ("Date/Time", 17.0),
        ("Status", 11.0),
        ("Method", 16.0),
        ("HEAD at time", 16.0),
    ],
    cli: CliTexts {
        report_title: "MrCodeReview Report",
        report_repo: "Repository",
        report_base: "Compared against",
        report_changed_files: "Changed files",
        report_files: "{n} file(s)",
        report_timestamp: "Run at (Unix time)",
        report_summary: "Summary",
        report_no_findings: "✓ **No findings in any aspect.**",
        report_total: "**Total findings**: {total} ({verified} verified, {unverified} unverified)",
        report_recipe_ok: "No findings (OK)",
        report_verified: "Verified",
        report_unverified: "⚠️ Unverified evidence (possible hallucination)",
        report_location: "Location",
        report_quote: "Quoted code",
        report_detail: "Details",
        recipes_heading: "Available review aspects:",
        recipes_col_title: "Title",
        recipes_col_source: "Source",
        recipes_src_builtin: "built-in",
        recipes_src_repo: "repository",
        review_start: "Review started: {n} file(s) changed (base: {base})",
        review_running: "[{i}/{n}] Reviewing: {title} ({id}) ...",
        review_done: "✅ Review complete: {path}",
        review_total: "Total findings: {total} ({verified} verified, {unverified} unverified)",
        review_history: "History saved to: {path} (shared with the GUI app)",
    },
};

#[cfg(test)]
mod builtin_recipe_tests {
    use super::*;
    use crate::recipe;

    #[test]
    fn both_languages_ship_the_same_builtin_ids_and_titles_map_across_languages() {
        let ja = recipe::list_with_lang(None, "ja");
        let en = recipe::list_with_lang(None, "en");
        let ids = |v: &[recipe::Recipe]| v.iter().map(|r| r.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&ja), ids(&en), "日本語と英語で、組み込みの観点の id がそろっていない");
        assert!(ids(&ja).iter().any(|i| i == "doc-claims") && ids(&ja).iter().any(|i| i == "tf-state"));
        for (j, e) in ja.iter().zip(en.iter()) {
            // 観点ファイルの見出しと、タイトルの表（i18n）が、同じ言語の中で一致する
            assert_eq!(Lang::Ja.texts().recipe_title(&j.title), j.title, "日本語のタイトルが表と食い違う: {}", j.id);
            assert_eq!(Lang::En.texts().recipe_title(&e.title), e.title, "英語のタイトルが表と食い違う: {}", e.id);
            // 言語をまたいで、タイトルが対応する（履歴の表示を、言語を切り替えても読めるように）
            assert_eq!(Lang::En.texts().recipe_title(&j.title), e.title, "日本語のタイトルが、英語に変換できない: {}", j.id);
            assert_eq!(Lang::Ja.texts().recipe_title(&e.title), j.title, "英語のタイトルが、日本語に変換できない: {}", e.id);
        }
    }

    /// 出力形式の雛形が、日本語・英語の両方で、同じ形のまま、JSON として読めること。
    /// （片方だけ直した・雛形を壊した、という事故を拾う）
    #[test]
    fn output_format_templates_parse_and_carry_the_same_instructions() {
        for (name, fmt, newline_hint) in [("ja", OUTPUT_FORMAT_JA, "改行"), ("en", OUTPUT_FORMAT_EN, "own line")] {
            let line = fmt.lines().find(|l| l.starts_with("{\"findings\"")).unwrap_or_else(|| panic!("{name}: 雛形の行が無い"));
            // 行番号の欄だけは、説明文が JSON の値の位置に直に入っている。null に置き換えて読む
            let (head, rest) = line.split_once("\"line\":").unwrap_or_else(|| panic!("{name}: line の欄が無い"));
            let (_, tail) = rest.split_once(",\"severity\"").unwrap_or_else(|| panic!("{name}: severity の欄が無い"));
            let json = format!("{head}\"line\":null,\"severity\"{tail}");
            let found = crate::findings::parse(&json).unwrap_or_else(|e| panic!("{name}: 雛形が読めない: {e}"));
            assert_eq!(found.len(), 1, "{name}");
            assert!(!found[0].title.is_empty() && !found[0].detail.is_empty() && !found[0].evidence.is_empty(), "{name}: 欄が欠けている");
            for key in ["`title`", "`detail`"] {
                assert!(fmt.contains(key), "{name}: {key} の書き方の指示が無い");
            }
            assert!(fmt.contains(newline_hint), "{name}: 1 文ごとの改行の指示が無い");
            let (ident_hint, effect_hint) = if name == "ja" { ("内部の用語", "ユーザーの画面や結果") } else { ("internal terms", "what the user sees") };
            assert!(fmt.contains(ident_hint) && fmt.contains(effect_hint), "{name}: 内部の用語を避ける・画面で起きることを書く、の指示が無い");
            assert!(fmt.contains("{\"findings\":[]}"), "{name}: 指摘が無いときの返し方が無い");
        }
    }
}
