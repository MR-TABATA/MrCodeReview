//! 応答から指摘を取り出し、引用が本当に差分にあるかを機械的に確かめる。
//!
//! モデルが引用したと言う 1 行が、送った差分に無ければ「根拠不一致」。幻覚の指摘をその場で炙り出す。
//! 見るのは文字列の一致だけで、指摘が正しいかどうかまでは判断しない。

use crate::snapshot::FileDiff;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Finding {
    pub file: String,
    pub line: Option<u32>,
    pub severity: String,
    pub title: String,
    pub detail: String,
    pub evidence: String,
    /// evidence が、その file の差分に実際にあるか。
    #[serde(default)]
    pub verified: bool,
    /// linter の規則名（ツール実行型の指摘だけ）。LLM の指摘は None。
    #[serde(default)]
    pub rule: Option<String>,
    /// 基準文書（設計書・仕様書）からの引用原文（設計書参照レビューのときだけ）。
    #[serde(default)]
    pub spec_quote: Option<String>,
    /// spec_quote が、参照した基準文書の本文に実際にあるか。
    #[serde(default)]
    pub spec_verified: Option<bool>,
}

fn str_field(v: &Value, key: &str) -> String {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("").trim().to_string()
}

/// 応答の文字列から、`findings` を持つ JSON のオブジェクトを探す。コードフェンスや前置きが付いていても拾う。
///
/// 最初の `{` から最後の `}` までを切ると、前後の説明文に `{...}` があるときに壊れる
/// （「{src/main.rs} の結果です … ※ {test} は除外」など）。そこで、各 `{` から順に、
/// JSON として 1 つ読めるかを試し、`findings` を持つ最初のものを採る。
fn find_findings_object(text: &str) -> Result<Value, String> {
    let mut saw_other_object = false;
    let mut first_error: Option<String> = None;
    for (i, _) in text.match_indices('{') {
        let mut de = serde_json::Deserializer::from_str(&text[i..]);
        match Value::deserialize(&mut de) {
            Ok(v) if v.get("findings").is_some() => return Ok(v),
            Ok(_) => saw_other_object = true,
            Err(e) => {
                first_error.get_or_insert_with(|| e.to_string());
            }
        }
    }
    if saw_other_object {
        Err("`findings` の配列が無い".into())
    } else if let Some(e) = first_error {
        Err(format!("JSON として読めない: {e}"))
    } else {
        Err("応答に JSON が見つからない".into())
    }
}

pub fn parse(text: &str) -> Result<Vec<Finding>, String> {
    let v = find_findings_object(text)?;
    let arr = v
        .get("findings")
        .and_then(|x| x.as_array())
        .ok_or("`findings` の配列が無い")?;
    Ok(arr
        .iter()
        .map(|f| Finding {
            file: str_field(f, "file"),
            line: f
                .get("line")
                .and_then(|x| x.as_u64().or_else(|| x.as_str().and_then(|s| s.trim().parse().ok())))
                .and_then(|n| u32::try_from(n).ok()),
            severity: {
                let s = str_field(f, "severity").to_lowercase();
                if s.is_empty() { "low".into() } else { s }
            },
            title: str_field(f, "title"),
            detail: str_field(f, "detail"),
            evidence: str_field(f, "evidence"),
            verified: false,
            rule: Some(str_field(f, "rule")).filter(|r| !r.is_empty()),
            spec_quote: Some(str_field(f, "spec_quote")).filter(|s| !s.is_empty()),
            spec_verified: None,
        })
        .collect())
}

pub(crate) fn same_file(finding: &str, path: &str) -> bool {
    let f = finding.trim_start_matches("./").trim_start_matches("a/").trim_start_matches("b/");
    !f.is_empty() && (f == path || path.ends_with(&format!("/{f}")) || f.ends_with(&format!("/{path}")))
}

/// 引用（evidence）が、その指摘のファイルの差分に含まれているかを確かめ、`verified` を埋める。
pub fn verify(findings: &mut [Finding], files: &[FileDiff]) {
    for f in findings.iter_mut() {
        let ev = f.evidence.trim();
        // `utils.ts` のように、同名のファイルが複数ありうる指摘は、どれか 1 つに引用があれば根拠あり
        // （最初に見つかった 1 つだけと照合すると、別のファイルへの指摘を、根拠不一致と誤判定する）
        f.verified = !ev.is_empty()
            && files
                .iter()
                .filter(|d| same_file(&f.file, &d.path))
                .any(|d| contains_evidence(&d.text, ev));
    }
}

fn contains_evidence(diff_text: &str, ev: &str) -> bool {
    diff_text.contains(ev) || ev.lines().map(str::trim).filter(|l| !l.is_empty()).all(|l| diff_text.contains(l))
}

/// 設計書からの引用（spec_quote）が、基準文書の本文に実在するかを確かめ、`spec_verified` を埋める。
///
/// 構想 §23: `spec_quote` も基準の文書の本文と文字列で照合し、無ければ「根拠不一致（Some(false)）」とする。
/// 飛び飛びの行の寄せ集めは認めず、本文に連続して実在する部分文字列のみを完全一致とする。
pub fn verify_spec(findings: &mut [Finding], spec_text: &str) {
    let trimmed_spec = spec_text.trim();
    for f in findings.iter_mut() {
        if let Some(quote) = f.spec_quote.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            f.spec_verified = Some(contains_quote(trimmed_spec, quote));
        } else {
            f.spec_verified = Some(false);
        }
    }
}

fn contains_quote(spec_text: &str, quote: &str) -> bool {
    let q = quote.trim();
    if q.is_empty() {
        return false;
    }
    if spec_text.contains(q) {
        return true;
    }
    // CRLF と LF の改行コード差分を吸収した連続一致
    let norm_spec = spec_text.replace("\r\n", "\n");
    let norm_quote = q.replace("\r\n", "\n");
    norm_spec.contains(&norm_quote)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str, text: &str) -> FileDiff {
        FileDiff { path: path.into(), bytes: text.len(), text: text.into() }
    }

    #[test]
    fn parses_plain_json() {
        let v = parse(r#"{"findings":[{"file":"a.js","line":3,"severity":"HIGH","title":"t","detail":"d","evidence":"e"}]}"#).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].line, Some(3));
        assert_eq!(v[0].severity, "high");
    }

    #[test]
    fn braces_in_the_surrounding_prose_do_not_break_the_parse() {
        // 前後の説明文に { } がある（最初の { から最後の } までを切ると壊れる）
        let text = "{src/main.rs} のレビュー結果です:\n```json\n{\"findings\":[{\"file\":\"a\",\"title\":\"x}y{\"}]}\n```\n※ {test} は除外";
        let v = parse(text).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].title, "x}y{", "文字列の中の波括弧も、壊さない");
        // 別のオブジェクトが先にあっても、findings を持つものを採る
        let v = parse(r#"メモ {"note": 1} 本体: {"findings":[{"file":"b"}]}"#).unwrap();
        assert_eq!(v[0].file, "b");
    }

    #[test]
    fn error_messages_tell_the_cases_apart() {
        assert!(parse("JSON なし").unwrap_err().contains("見つからない"));
        assert!(parse("{bad").unwrap_err().contains("JSON として読めない"));
        assert!(parse(r#"{"other":1}"#).unwrap_err().contains("`findings` の配列が無い"));
        assert!(parse(r#"{"findings": 5}"#).unwrap_err().contains("`findings` の配列が無い"));
    }

    #[test]
    fn same_named_files_are_not_mixed_up_when_verifying() {
        // pkg_a/utils.ts と pkg_b/utils.ts。モデルが file: "utils.ts" とだけ返し、引用は pkg_b にある
        let files = vec![file("pkg_a/utils.ts", "+const a = 1;\n"), file("pkg_b/utils.ts", "+danger(x);\n")];
        let mut v = vec![Finding { file: "utils.ts".into(), line: None, severity: "low".into(), title: "".into(), detail: "".into(), evidence: "danger(x);".into(), verified: false, rule: None, ..Default::default() }];
        verify(&mut v, &files);
        assert!(v[0].verified, "最初に見つかった pkg_a だけと照合して、根拠不一致にしない");
        // どちらにも無い引用は、これまでどおり根拠不一致
        let mut w = vec![Finding { file: "utils.ts".into(), line: None, severity: "low".into(), title: "".into(), detail: "".into(), evidence: "nowhere();".into(), verified: false, rule: None, ..Default::default() }];
        verify(&mut w, &files);
        assert!(!w[0].verified);
    }

    #[test]
    fn parses_rule_when_given_and_leaves_it_none_otherwise() {
        let v = parse(r#"{"findings":[{"file":"a","rule":"no-var"},{"file":"b","rule":""},{"file":"c"}]}"#).unwrap();
        assert_eq!(v[0].rule.as_deref(), Some("no-var"));
        assert_eq!(v[1].rule, None);
        assert_eq!(v[2].rule, None);
    }

    #[test]
    fn parses_through_code_fence_and_preamble() {
        let v = parse("結果です\n```json\n{\"findings\":[]}\n```\n").unwrap();
        assert!(v.is_empty());
    }

    #[test]
    fn line_may_be_string_or_null_or_garbage() {
        let v = parse(r#"{"findings":[{"file":"a","line":"12"},{"file":"b","line":null},{"file":"c","line":-5},{"file":"d"}]}"#).unwrap();
        assert_eq!(v[0].line, Some(12));
        assert_eq!(v[1].line, None);
        assert_eq!(v[2].line, None);
        assert_eq!(v[3].line, None);
        assert_eq!(v[3].severity, "low");
    }

    #[test]
    fn errors_are_reported_not_swallowed() {
        assert!(parse("JSON なし").is_err());
        assert!(parse("{bad").is_err());
        assert!(parse(r#"{"other":1}"#).is_err());
    }

    #[test]
    fn verify_accepts_real_quote_and_rejects_invented() {
        let files = vec![file("src/db.js", "+  db.query(\"SELECT * FROM u WHERE id=\" + id);\n")];
        let mut v = vec![
            Finding { file: "src/db.js".into(), line: Some(1), severity: "high".into(), title: "".into(), detail: "".into(), evidence: "db.query(\"SELECT * FROM u WHERE id=\" + id);".into(), verified: false, rule: None, ..Default::default() },
            Finding { file: "src/db.js".into(), line: Some(1), severity: "high".into(), title: "".into(), detail: "".into(), evidence: "eval(userInput)".into(), verified: false, rule: None, ..Default::default() },
            Finding { file: "src/db.js".into(), line: Some(1), severity: "high".into(), title: "".into(), detail: "".into(), evidence: "".into(), verified: false, rule: None, ..Default::default() },
            Finding { file: "other.js".into(), line: Some(1), severity: "high".into(), title: "".into(), detail: "".into(), evidence: "db.query".into(), verified: false, rule: None, ..Default::default() },
        ];
        verify(&mut v, &files);
        assert!(v[0].verified);
        assert!(!v[1].verified, "差分に無い引用は根拠不一致");
        assert!(!v[2].verified, "引用が空なら根拠不一致");
        assert!(!v[3].verified, "別ファイルの差分にしか無い引用は根拠不一致");
    }

    #[test]
    fn verify_matches_path_variants() {
        let files = vec![file("src/a.js", "+x = 1\n")];
        let mut v = vec![
            Finding { file: "b/src/a.js".into(), line: None, severity: "low".into(), title: "".into(), detail: "".into(), evidence: "x = 1".into(), verified: false, rule: None, ..Default::default() },
            Finding { file: "a.js".into(), line: None, severity: "low".into(), title: "".into(), detail: "".into(), evidence: "x = 1".into(), verified: false, rule: None, ..Default::default() },
        ];
        verify(&mut v, &files);
        assert!(v[0].verified && v[1].verified);
    }

    #[test]
    fn verify_spec_checks_exact_quotes() {
        let spec = "行A: 認証仕様\n行B: 中間の処理\n行C: 自動延長は行わない。";
        let mut v = vec![
            Finding {
                file: "src/auth.rs".into(),
                spec_quote: Some("行A: 認証仕様".into()),
                ..Default::default()
            },
            Finding {
                file: "src/auth.rs".into(),
                spec_quote: Some("存在しない仕様の引用".into()),
                ..Default::default()
            },
            Finding {
                file: "src/auth.rs".into(),
                spec_quote: None,
                ..Default::default()
            },
            Finding {
                file: "src/auth.rs".into(),
                spec_quote: Some("   ".into()),
                ..Default::default()
            },
            // 離れた行を並べた偽の引用（行A と 行C）は不一致（false）になること
            Finding {
                file: "src/auth.rs".into(),
                spec_quote: Some("行A: 認証仕様\n行C: 自動延長は行わない。".into()),
                ..Default::default()
            },
        ];
        verify_spec(&mut v, spec);
        assert_eq!(v[0].spec_verified, Some(true), "完全一致する引用は照合成功");
        assert_eq!(v[1].spec_verified, Some(false), "設計書に無い引用は不一致");
        assert_eq!(v[2].spec_verified, Some(false), "引用がないものは不一致");
        assert_eq!(v[3].spec_verified, Some(false), "空白のみの引用は不一致");
        assert_eq!(v[4].spec_verified, Some(false), "飛び飛びの行を合成した引用は不一致");
    }

    #[test]
    fn parse_and_verify_spec_quotes_end_to_end() {
        let json_text = r#"```json
{
  "findings": [
    {
      "file": "src/auth.rs",
      "line": 10,
      "severity": "high",
      "title": "セッション延長の仕様違反",
      "detail": "仕様では自動延長不可と規定されています",
      "evidence": "session.extend()",
      "spec_quote": "自動延長は不可とする。"
    },
    {
      "file": "src/auth.rs",
      "line": 20,
      "severity": "medium",
      "title": "でっち上げの引用",
      "detail": "架空のルール",
      "evidence": "let x = 1;",
      "spec_quote": "設計書に存在しない架空の文言"
    },
    {
      "file": "src/auth.rs",
      "line": 30,
      "severity": "low",
      "title": "空の引用",
      "detail": "引用なし",
      "evidence": "let y = 2;",
      "spec_quote": ""
    },
    {
      "file": "src/auth.rs",
      "line": 40,
      "severity": "low",
      "title": "引用欄なし",
      "detail": "spec_quote フィールド自体がない",
      "evidence": "let z = 3;"
    }
  ]
}
```"#;
        let mut findings = parse(json_text).unwrap();
        assert_eq!(findings.len(), 4);
        assert_eq!(findings[0].spec_quote.as_deref(), Some("自動延長は不可とする。"));
        assert_eq!(findings[1].spec_quote.as_deref(), Some("設計書に存在しない架空の文言"));
        assert_eq!(findings[2].spec_quote, None, "空文字は None に正規化される");
        assert_eq!(findings[3].spec_quote, None, "欄なしは None になる");

        let spec_content = "# 認証仕様\nセッションは発行後15分で破棄し、自動延長は不可とする。\n";
        verify_spec(&mut findings, spec_content);

        assert_eq!(findings[0].spec_verified, Some(true), "実在する原文引用は true");
        assert_eq!(findings[1].spec_verified, Some(false), "でっち上げ引用は false");
        assert_eq!(findings[2].spec_verified, Some(false), "空引用は false");
        assert_eq!(findings[3].spec_verified, Some(false), "引用欄なしは false");
    }
}
