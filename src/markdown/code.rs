use super::{Line, LineStyle, Span, SpanStyle};
use crate::markdown::mermaid::{render_mermaid, Fallback};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use crate::markdown::wrap::line_width;

/// 펜스 언어 별칭 → syntect 가 아는 확장자(요구사항 10.7). `rs`/`py`/`sh`/`js` 는
/// syntect 기본 번들의 실제 확장자라 이미 되지만, 흔한 전체 이름(`rust`/`python`/
/// `javascript`)은 기본 번들에 그 이름의 확장자가 없어 그대로 두면 평문으로
/// 떨어진다 — 실측: `ps.find_syntax_by_extension("rust")` 은 `None`.
/// `ts`/`typescript` 는 기본 번들에 TypeScript 문법 자체가 없어(`None`) JS 로
/// 근사 강조한다(문법적으로 JS 의 상위집합이라 대부분의 토큰이 겹친다).
fn resolve_lang_alias(lang: &str) -> &str {
    match lang {
        "rust" => "rs",
        "python" => "py",
        "javascript" => "js",
        "typescript" | "ts" => "js",
        "shell" => "sh",
        other => other,
    }
}

pub fn push_code_block(
    lang: &str,
    code: &str,
    width: u16,
    out: &mut Vec<Line>,
    plain: &mut Vec<String>,
) {
    if lang == "mermaid" {
        match render_mermaid(code, width) {
            Ok(lines) => {
                for line in lines {
                    out.push(Line {
                        indent: 0,
                        spans: vec![Span { text: line.clone(), style: SpanStyle::Plain }],
                        style: Some(LineStyle::Plain),
                    });
                    plain.push(line);
                }
            }
            Err(fallback) => {
                let label = match fallback {
                    Fallback::Empty { kind } => format!("Mermaid (Empty): {}", kind),
                    Fallback::Overflow { kind } => format!("Mermaid (Overflow): {}", kind),
                };
                
                out.push(Line {
                    indent: 0,
                    spans: vec![Span { text: label.clone(), style: SpanStyle::Code }],
                    style: Some(LineStyle::Plain),
                });
                plain.push(label);

                for line in code.lines() {
                    let mut text = line.to_string();
                    if line_width(&text) > width as usize {
                        let limit = width as usize - 1;
                        let truncated: String = text.chars().take(limit).collect();
                        text = format!("{}…", truncated);
                    }
                    out.push(Line {
                        indent: 0,
                        spans: vec![Span { text: text.clone(), style: SpanStyle::Code }],
                        style: Some(LineStyle::Plain),
                    });
                    plain.push(text);
                }
            }
        }
        return;
    }

    // 10.7: 탭은 4칸으로 — 하이라이팅·폭 계산 전에 먼저 펼친다.
    let code = code.replace('\t', "    ");

    let ps = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let resolved = resolve_lang_alias(lang);
    let syntax = ps.find_syntax_by_extension(resolved).unwrap_or_else(|| ps.find_syntax_plain_text());
    let theme = ts.themes.get("base16-ocean.theme").unwrap_or_else(|| ts.themes.values().next().unwrap());
    let mut h = HighlightLines::new(syntax, theme);

    for line in code.lines() {
        let ranges = h.highlight_line(line, &ps).unwrap();
        let mut spans = Vec::new();
        
        for (style, text) in ranges {
            let span_style = if style.font_style.is_empty() {
                SpanStyle::Code 
            } else {
                SpanStyle::Plain
            };
            
            spans.push(Span { 
                text: text.to_string(), 
                style: span_style 
            });
        }

        let mut final_spans = Vec::new();
        let mut current_w = 0;
        for s in spans {
            let w = line_width(&s.text);
            if current_w + w > width as usize {
                let remaining = width as usize - current_w;
                if remaining > 0 {
                    let truncated: String = s.text.chars().take(remaining - 1).collect();
                    final_spans.push(Span { 
                        text: format!("{}…", truncated), 
                        style: s.style 
                    });
                }
                break;
            }
            final_spans.push(s);
            current_w += w;
        }

        out.push(Line {
            indent: 0,
            spans: final_spans,
            style: Some(LineStyle::Plain),
        });
        plain.push(line.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::{Line, Span, SpanStyle, LineStyle};

    #[test]
    fn test_code_truncation() {
        let mut out = Vec::new();
        let mut plain = Vec::new();
        let width = 10;
        let code = "this is a very long line that exceeds width";
        
        push_code_block("text", code, width, &mut out, &mut plain);
        
        assert!(out[0].spans[0].text.ends_with('…'));
        assert!(line_width(&out[0].spans[0].text) <= 10);
    }

    #[test]
    fn test_mermaid_fallback_empty() {
        let mut out = Vec::new();
        let mut plain = Vec::new();
        let width = 80;
        let code = "graph TB\n  %% just a comment";

        push_code_block("mermaid", code, width, &mut out, &mut plain);

        assert_eq!(out[0].spans[0].text, "Mermaid (Empty): flowchart");
        assert_eq!(out[1].spans[0].text, "graph TB");
        assert_eq!(out[2].spans[0].text, "  %% just a comment");
    }

    #[test]
    fn test_lang_alias_resolves_to_real_tokenized_syntax() {
        // 10.7: rust/python/typescript(ts) 같은 흔한 별칭이 실제 구문별 토큰화로
        // 이어지는지 실측 신호로 확인한다 — plain-text 폴백은 줄 전체가 토큰 1개로
        // 뭉쳐 나오고(실측: `find_syntax_by_extension("doesnotexist")` 로 확인),
        // 실제 언어 문법은 키워드·식별자·구두점이 여러 스팬으로 쪼개진다.
        let width = 80;
        for (lang, code) in [
            ("rust", "fn main() { let x = 1; }"),
            ("python", "def f(x):"),
            ("typescript", "const x: number = 1;"),
            ("ts", "const x: number = 1;"),
        ] {
            let mut out = Vec::new();
            let mut plain = Vec::new();
            push_code_block(lang, code, width, &mut out, &mut plain);
            assert!(
                out[0].spans.len() > 1,
                "{lang} fell back to plain-text tokenization (1 span) instead of real syntax"
            );
        }
    }

    #[test]
    fn test_tab_expands_to_four_spaces() {
        // 10.7: 탭은 4칸.
        let mut out = Vec::new();
        let mut plain = Vec::new();
        push_code_block("text", "\tindented", 80, &mut out, &mut plain);
        assert_eq!(plain[0], "    indented");
    }

    #[test]
    fn test_syntax_highlighting_basic() {
        let mut out = Vec::new();
        let mut plain = Vec::new();
        let width = 80;
        let code = "fn main() {}";
        
        push_code_block("rust", code, width, &mut out, &mut plain);
        
        let has_code_style = out[0].spans.iter().any(|s| s.style == SpanStyle::Code);
        assert!(has_code_style, "Should have syntax highlighting applied");
    }
}
