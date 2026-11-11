//! レポートの Excel（.xlsx）出力。`report::Report` を、4 つのシートにする。
//!
//! - 「概要」: 期間・件数・トークンなど。
//! - 「レビュー」: 1 行が、レビュー 1 回。
//! - 「指摘」: 1 行が、指摘 1 件（現在の仕分け・対応済み・対応メモ・その後）。フィルターで絞れる。
//! - 「仕分けの履歴」: 1 行が、仕分けの変更 1 回（いつ・何にしたか）。
//!
//! 文字は、必ず**文字列として**書く（`=` や `+` で始まる文章が、数式として動かないように）。

use crate::report::{self, Report};
use rust_xlsxwriter::{Color, Format, FormatAlign, Workbook, Worksheet, XlsxError};
use std::path::Path;

/// 1 つのセルに入る文字数の上限は 32,767。余裕を見て切る。
const CELL_MAX: usize = 30_000;

fn cap(s: &str) -> String {
    if s.chars().count() <= CELL_MAX {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(CELL_MAX).collect();
        t.push_str("…（長いので、ここで切りました）");
        t
    }
}

fn err(e: XlsxError) -> String {
    format!("Excel のファイルを作れません: {e}")
}

struct Sheet<'a> {
    ws: &'a mut Worksheet,
    body: Format,
    row: u32,
}

impl<'a> Sheet<'a> {
    fn new(ws: &'a mut Worksheet, name: &str, headers: &[(&str, f64)]) -> Result<Sheet<'a>, String> {
        let head = Format::new().set_bold().set_background_color(Color::RGB(0xE8ECF4)).set_text_wrap().set_align(FormatAlign::Top).set_border_bottom(rust_xlsxwriter::FormatBorder::Thin);
        let body = Format::new().set_text_wrap().set_align(FormatAlign::Top);
        ws.set_name(name).map_err(err)?;
        for (c, (h, w)) in headers.iter().enumerate() {
            ws.write_string_with_format(0, c as u16, *h, &head).map_err(err)?;
            ws.set_column_width(c as u16, *w).map_err(err)?;
        }
        if !headers.is_empty() {
            ws.set_freeze_panes(1, 0).map_err(err)?;
        }
        Ok(Sheet { ws, body, row: 1 })
    }

    fn text(&mut self, col: u16, s: &str) -> Result<(), String> {
        self.ws.write_string_with_format(self.row, col, cap(s), &self.body).map_err(err)?;
        Ok(())
    }

    fn num(&mut self, col: u16, n: u64) -> Result<(), String> {
        self.ws.write_number_with_format(self.row, col, n as f64, &self.body).map_err(err)?;
        Ok(())
    }

    fn next(&mut self) {
        self.row += 1;
    }

    /// フィルターを付ける（データがあるときだけ）。
    fn finish(&mut self, last_col: u16) -> Result<(), String> {
        if self.row > 1 {
            self.ws.autofilter(0, 0, self.row - 1, last_col).map_err(err)?;
        }
        Ok(())
    }
}

pub fn write(r: &Report, path: &Path) -> Result<(), String> {
    let lang = crate::i18n::Lang::from_code(&r.lang).unwrap_or_default();
    let t = lang.texts();
    let tz = r.tz_offset_min;
    let mut wb = Workbook::new();

    // ── 概要 ──
    {
        let ws = wb.add_worksheet();
        let mut s = Sheet::new(ws, t.xlsx_sheet_summary, t.xlsx_summary_headers)?;
        for (k, v) in report::summary_lines(r) {
            s.text(0, &k)?;
            s.text(1, &v)?;
            s.next();
        }
        s.text(0, t.sum_notice)?;
        s.text(1, &t.notice(&r.app_name))?;
    }

    // ── レビュー ──
    {
        let mut headers: Vec<(&str, f64)> = t.xlsx_review_headers.to_vec();
        if r.include_tokens {
            headers.extend_from_slice(t.xlsx_review_token_headers);
        }
        headers.push(t.xlsx_review_notes_header);
        let ws = wb.add_worksheet();
        let mut s = Sheet::new(ws, t.xlsx_sheet_reviews, &headers)?;
        for v in &r.reviews {
            let (fix, wait, dismiss, new, done) = v.counts();
            let mut c: u16 = 0;
            s.num(c, v.n as u64)?;
            c += 1;
            s.text(c, &report::fmt_datetime(v.created_at, tz))?;
            c += 1;
            s.text(c, &v.recipe_title)?;
            c += 1;
            s.text(c, &v.provider_label)?;
            c += 1;
            s.text(c, &v.model)?;
            c += 1;
            s.text(c, &report::base_or_none_for(v, lang))?;
            c += 1;
            s.text(c, report::result_label_for(v, lang))?;
            c += 1;
            s.ws.write_number_with_format(s.row, c, (v.duration_ms as f64 / 100.0).round() / 10.0, &s.body).map_err(err)?;
            c += 1;
            for n in [v.findings.len(), fix, wait, dismiss, new, done] {
                s.num(c, n as u64)?;
                c += 1;
            }
            s.text(c, &v.head.chars().take(10).collect::<String>())?;
            c += 1;
            s.text(c, &v.diff_hash.chars().take(10).collect::<String>())?;
            c += 1;
            s.text(c, &v.tag)?;
            c += 1;
            if r.include_tokens {
                for tok in [v.usage.input_tokens, v.usage.output_tokens, v.usage.cache_read_tokens, v.usage.cache_write_tokens] {
                    match tok {
                        Some(n) => s.num(c, n)?,
                        None => s.text(c, t.unreported_cap)?,
                    }
                    c += 1;
                }
            }
            let mut notes: Vec<String> = Vec::new();
            if let Some(e) = &v.error {
                notes.push(t.xlsx_failed_note(e));
            }
            if let Some(e) = &v.parse_error {
                notes.push(t.xlsx_parse_err_note(e));
            }
            notes.extend(v.run_notes.iter().cloned());
            s.text(c, &notes.join("\n"))?;
            s.next();
        }
        let last = (headers.len() - 1) as u16;
        s.finish(last)?;
    }

    // ── 指摘 ──
    {
        let mut headers: Vec<(&str, f64)> = t.xlsx_finding_headers_head.to_vec();
        if r.include_evidence {
            headers.push(t.xlsx_finding_quotation_header);
        }
        headers.extend_from_slice(t.xlsx_finding_headers_tail);
        let ws = wb.add_worksheet();
        let mut s = Sheet::new(ws, t.xlsx_sheet_findings, &headers)?;
        for v in &r.reviews {
            for f in &v.findings {
                let mut c: u16 = 0;
                s.num(c, v.n as u64)?;
                c += 1;
                s.text(c, &report::fmt_datetime(v.created_at, tz))?;
                c += 1;
                s.text(c, &v.recipe_title)?;
                c += 1;
                s.num(c, f.n as u64)?;
                c += 1;
                s.text(c, &f.file)?;
                c += 1;
                match f.line {
                    Some(l) => s.num(c, l as u64)?,
                    None => s.text(c, "")?,
                }
                c += 1;
                s.text(c, report::severity_label(&f.severity))?;
                c += 1;
                s.text(c, f.rule.as_deref().unwrap_or(""))?;
                c += 1;
                s.text(c, &f.title)?;
                c += 1;
                s.text(c, &f.detail)?;
                c += 1;
                if r.include_evidence {
                    s.text(c, &f.evidence)?;
                    c += 1;
                }
                s.text(c, if f.verified { t.ev_found_short } else { t.ev_not_in_diff_xlsx })?;
                c += 1;
                s.text(c, t.state_label(&f.state))?;
                c += 1;
                s.text(c, if f.note.done_at.is_some() { t.xlsx_done_mark } else { "" })?;
                c += 1;
                // 時刻が無いもの（この機能より前の記録）は、作らず、「記録なし」と書く
                s.text(c, &match (f.state.as_str(), f.note.triage_at) {
                    ("new", _) => String::new(),
                    (_, Some(at)) => report::fmt_datetime(at, tz),
                    (_, None) => t.no_record.to_string(),
                })?;
                c += 1;
                s.text(c, match f.note.triage_source.as_deref() {
                    Some("carry") => t.xlsx_source_carry,
                    Some(_) => t.xlsx_source_manual,
                    None => "",
                })?;
                c += 1;
                s.text(c, &f.note.done_at.map(|a| report::fmt_datetime(a, tz)).unwrap_or_default())?;
                c += 1;
                s.text(c, f.note.done_head.as_deref().map(|h| h.chars().take(10).collect::<String>()).as_deref().unwrap_or(""))?;
                c += 1;
                s.text(c, &f.note.memo)?;
                c += 1;
                s.text(c, &f.later.as_ref().map(|l| report::later_text_for(l, tz, lang)).unwrap_or_default())?;
                c += 1;
                s.text(c, &f.agent.as_ref().map(|a| report::agent_text_for(a, tz, lang)).unwrap_or_default())?;
                c += 1;
                s.text(c, &f.tag)?;
                s.next();
            }
        }
        let last = (headers.len() - 1) as u16;
        s.finish(last)?;
    }

    // ── 仕分けの履歴 ──
    if r.include_log {
        let ws = wb.add_worksheet();
        let mut s = Sheet::new(ws, t.xlsx_sheet_history, t.xlsx_history_headers)?;
        for v in &r.reviews {
            for f in &v.findings {
                for l in &f.log {
                    s.num(0, v.n as u64)?;
                    s.text(1, &report::fmt_datetime(v.created_at, tz))?;
                    s.text(2, &v.recipe_title)?;
                    s.num(3, f.n as u64)?;
                    s.text(4, &f.title)?;
                    s.text(5, &report::fmt_datetime(l.at, tz))?;
                    s.text(6, if l.state == "new" { t.state_reverted_new } else { t.state_label(&l.state) })?;
                    s.text(7, if l.source == crate::triage::SRC_CARRY { t.xlsx_source_carry } else { t.xlsx_source_manual })?;
                    s.text(8, l.head.as_deref().map(|h| h.chars().take(10).collect::<String>()).as_deref().unwrap_or(""))?;
                    s.next();
                }
            }
        }
        s.finish(8)?;
    }

    wb.save(path).map_err(err)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cell_at_the_limit_is_kept_whole_and_one_over_is_cut_with_a_note() {
        let at = "a".repeat(CELL_MAX);
        assert_eq!(cap(&at), at, "上限ちょうどは、そのまま");
        let over = "a".repeat(CELL_MAX + 1);
        let cut = cap(&over);
        assert!(cut.starts_with(&"a".repeat(CELL_MAX)) && cut.ends_with("（長いので、ここで切りました）"));
        assert!(cut.chars().count() < 32_767, "Excel のセルの上限（32,767 文字）に収まる: {}", cut.chars().count());
    }

    #[test]
    fn the_limit_counts_characters_not_bytes_so_japanese_text_is_not_cut_early_or_mid_character() {
        let jp = "あ".repeat(CELL_MAX); // 90,000 バイトだが、30,000 文字
        assert_eq!(cap(&jp), jp, "文字数で数える（バイト数なら、切られてしまう）");
        let long = "あ".repeat(CELL_MAX + 5);
        let cut = cap(&long);
        assert!(cut.starts_with(&"あ".repeat(CELL_MAX)), "文字の途中では切らない");
        assert!(cut.chars().count() < 32_767);
    }

    #[test]
    fn a_very_long_value_still_writes_a_valid_file() {
        let d = std::env::temp_dir().join(format!("mrcr-xlsx-long-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let mut wb = Workbook::new();
        {
            let ws = wb.add_worksheet();
            let mut sheet = Sheet::new(ws, "長い", &[("内容", 20.0)]).unwrap();
            sheet.text(0, &"x".repeat(CELL_MAX * 2)).unwrap();
        }
        let out = d.join("long.xlsx");
        wb.save(&out).expect("上限を超える値でも、ファイルを作れる（切って書く）");
        assert!(std::fs::read(&out).unwrap().starts_with(b"PK"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
