//! Task 10.5 — glow 3.0 대비 스냅샷 (requirement 10.8).
//!
//! 두 갈래로 나뉜다:
//! 1. `glow`(charmbracelet/glow) 가 설치돼 있을 때만, 이 저장소의 실제
//!    `.kiro/specs/*/design.md` 전체를 `glow -s dark -w 100` 과 `m`(이 크레이트의
//!    렌더러) 양쪽으로 렌더해 `tests/snapshots/glow-vs-m/<spec>/{glow,m}.txt` 에
//!    나란히 저장한다 — 사람이 검토할 대조 자료다. 미설치 환경에서는 건너뛴다.
//! 2. glow 설치 여부와 무관하게 항상 도는 자동 단정 — 10.1~10.7 이 실제로
//!    발화하는지 확인한다.
//!
//! ⚠️ **의도적 이탈, 숨기지 않는다**: 실제 `design.md` 8개를 전수 조사한 결과
//! (`grep` 실측) 인용문(`>`)과 `rust`/`json`/`toml` 코드 펜스는 전부에 있지만,
//! **체크박스·링크·이미지·각주는 단 하나도 없다.** 자동 단정 절이 "10.1~10.7
//! 기호...존재"라고 못박았으므로, 실제 문서에 없는 요소를 억지로 문서에 끼워
//! 넣는 대신(진짜 코퍼스를 오염시킨다) — glow 비교 대상은 실제 `design.md` 그대로
//! 두고, 체크박스/링크/이미지/각주 존재 단정은 이 파일 안의 작은 전용 스모크
//! 픽스처(`FEATURE_SMOKE_MD`, 이 파일에서만 씀 — 공유 골든 픽스처를 건드리지
//! 않는다)로 확인한다. 그 문서들에 실제로 있는 요소(헤딩 밑줄·목록 기호·인용
//! 막대·코드 강조)는 real 코퍼스에서 직접 확인한다.

use spec_viewer::markdown;
use spec_viewer::markdown::code;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const WIDTH: u16 = 100;

/// 10.1~10.7 요소를 전부 담은 작은 전용 픽스처 — 실제 design.md 에는 없는
/// 체크박스·링크·이미지·각주 존재를 확인하기 위한 것. 골든 스냅샷(`tests/
/// fixtures/golden/`)과는 별개이며 그쪽에 영향을 주지 않는다.
const FEATURE_SMOKE_MD: &str = "\
# 제목1\n\
## 제목2\n\
### 제목3\n\n\
- 목록 A\n\
  - 목록 A-1\n\
    - 목록 A-2\n\
- [x] 완료 항목\n\
- [ ] 미완료 항목\n\n\
> 인용문 한 줄\n\n\
참고는 [여기](https://example.com) 그리고 ![그림](./x.png) 를 보라[^note].\n\n\
[^note]: 각주 본문\n\n\
```rust\nfn main() {\n\tprintln!(\"tab\");\n}\n```\n";

fn design_md_files() -> Vec<PathBuf> {
    // This crate carries its own `.kiro/` specs (moved in from the
    // archgenworks monorepo, commit 0ab79a3) rather than assuming it's
    // checked out one level under an external workspace's `.kiro/`.
    let specs_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(".kiro/specs");
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(&specs_dir) {
        for entry in entries.flatten() {
            let p = entry.path().join("design.md");
            if p.is_file() {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// 마크다운 원문에서 펜스 코드 블록의 (언어, 본문)만 뽑아낸다 — 전체 문서를
/// 렌더하지 않으므로 표·인용·인라인 코드가 섞여 들어올 수 없다(위 주석 참조).
/// 들여쓰기 코드블록·중첩 펜스 등은 다루지 않는 단순 스캐너 — 이 저장소
/// design.md 들이 실제로 쓰는 백틱 3개 펜스만 대상으로 하면 충분하다.
fn extract_fenced_code_blocks(src: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut lines = src.lines();
    while let Some(line) = lines.next() {
        let Some(lang) = line.strip_prefix("```") else { continue };
        let lang = lang.trim().to_string();
        let mut body_lines = Vec::new();
        for body_line in lines.by_ref() {
            if body_line.trim_end() == "```" {
                break;
            }
            body_lines.push(body_line);
        }
        out.push((lang, body_lines.join("\n")));
    }
    out
}

fn glow_available() -> bool {
    Command::new("glow")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[test]
fn glow_vs_m_snapshots_saved_when_glow_installed() {
    if !glow_available() {
        eprintln!("glow not installed — skipping snapshot save (10.8 is opt-in per design.md Testing Strategy)");
        return;
    }
    let files = design_md_files();
    assert!(!files.is_empty(), "no design.md files found under .kiro/specs");

    let out_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots/glow-vs-m");
    fs::create_dir_all(&out_root).unwrap();

    for design_path in &files {
        let spec_name = design_path
            .parent()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let src = fs::read_to_string(design_path).unwrap();

        let glow_out = Command::new("glow")
            .args(["-s", "dark", "-w", &WIDTH.to_string()])
            .arg(design_path)
            .output()
            .unwrap_or_else(|e| panic!("failed to run glow on {design_path:?}: {e}"));
        assert!(glow_out.status.success(), "glow exited non-zero for {design_path:?}");
        let glow_text = String::from_utf8_lossy(&glow_out.stdout).to_string();

        let m_text = markdown::render(&src, WIDTH).plain.join("\n");

        let spec_dir = out_root.join(&spec_name);
        fs::create_dir_all(&spec_dir).unwrap();
        fs::write(spec_dir.join("glow.txt"), &glow_text).unwrap();
        fs::write(spec_dir.join("m.txt"), &m_text).unwrap();
    }
}

#[test]
fn real_design_md_corpus_exercises_heading_rule_list_bullet_quote_bar_code_highlight() {
    // glow 설치 여부와 무관하게 항상 돈다. 실제 design.md 8개 전수에서 10.1의
    // 밑줄·목록 기호·인용 막대와 10.7의 코드 강조가 실제로 발화하는지 확인한다
    // — 합성 픽스처가 아니라 이 저장소의 진짜 스펙 문서로.
    let files = design_md_files();
    assert!(!files.is_empty(), "no design.md files found under .kiro/specs — corpus is empty");

    let mut any_heading_rule = false;
    let mut any_list_bullet = false;
    let mut any_quote_bar = false;
    let mut any_multi_token_code_line = false;

    for path in &files {
        let src = fs::read_to_string(path).unwrap();
        let rendered = markdown::render(&src, WIDTH);

        if rendered
            .lines
            .iter()
            .any(|l| l.spans.iter().any(|s| s.text.chars().all(|c| c == '━' || c == '─') && !s.text.is_empty()))
        {
            any_heading_rule = true;
        }
        for line in &rendered.plain {
            let t = line.trim_start();
            if t.starts_with('•') || t.starts_with('◦') || t.starts_with('▪') {
                any_list_bullet = true;
            }
            if t.starts_with('\u{2502}') {
                any_quote_bar = true;
            }
        }
        // 코드 펜스가 실제로 구문 강조(진짜 토큰화)됐는지 — 10.7.
        //
        // 세 번의 리뷰가 각각 다른 혼입원을 잡았다: 1차는 `spans.len() > 3`
        // 가 표 행에서도 항상 참이라는 것, 2차는 `LineStyle::Table` 만 제외해도
        // 본문 인라인 코드(백틱 하나)가 같은 신호를 낸다는 것, 3차는 펜스를
        // 문서에서 분리해 단독 호출해도 **언어 미지정/미인식 펜스가 떨어지는
        // syntect 의 plain-text 폴백조차 `SpanStyle::Code` 를 낸다는 것**
        // (code.rs 의 `font_style.is_empty() → Code` 판정이 폴백 토큰 1개에도
        // 그대로 걸린다 — 실측: 이 저장소 8개 문서 전부가 언어 미지정 펜스
        // (디렉터리 트리 그림)를 하나씩 갖고 있고, 전부 이 경로로 "통과"했었다).
        //
        // 그래서 두 가지를 더 좁힌다: (a) 언어가 없는 펜스는 애초에 구문강조
        // 대상이 아니므로 건너뛴다(mermaid 와 같은 이유), (b) `SpanStyle::Code`
        // 존재가 아니라 **한 줄에 스팬이 2개 이상**인지로 본다 — code.rs 자신의
        // 실측(`test_lang_alias_resolves_to_real_tokenized_syntax`)이 이미
        // 증명했듯 plain-text 폴백은 줄 전체가 항상 스팬 1개로 뭉친다.
        //
        // ⚠️ **4차 리뷰가 잡은 것**: 이 코퍼스엔 `toml` 펜스도 1개 있는데,
        // syntect 기본 번들엔 TOML 문법이 아예 없다(`find_syntax_by_extension
        // ("toml")` 이 `None`) — 그래서 `toml`(non-empty lang)도 조용히
        // plain-text 폴백으로 떨어져 스팬 1개로 남는다(실측: 77행 전부). 아래
        // 단정은 **rust·json 만으로 통과**하고(87/87, 5/5) toml 은 못 잡는다 —
        // 이건 회귀가 아니라(요구사항 10.7 은 toml 을 약속하지 않는다) 실측
        // 사실이며, "실제로 발화하는지"를 이 세 언어 전부에 대해 증명한다고
        // 주장하지 않는다.
        for (lang, body) in extract_fenced_code_blocks(&src) {
            if lang.is_empty() || lang == "mermaid" {
                continue; // 언어 미지정(디렉터리 트리 등)·mermaid(9.5 로 별도 검증) — 구문강조 대상 아님.
            }
            let mut lines = Vec::new();
            let mut plain_lines = Vec::new();
            code::push_code_block(&lang, &body, WIDTH, &mut lines, &mut plain_lines);
            if lines.iter().any(|l| l.spans.len() > 1) {
                any_multi_token_code_line = true;
            }
        }
    }

    assert!(any_heading_rule, "no design.md produced an H1/H2 heading rule line — 10.1 unproven on real corpus");
    assert!(any_list_bullet, "no design.md produced a themed list bullet — 10.1/10.2 unproven on real corpus");
    assert!(any_quote_bar, "no design.md produced a quote bar line — 10.1 unproven on real corpus (7/8 files contain blockquotes per manual grep — spec-viewer/design.md's only `>` occurrences are mermaid arrows, not blockquotes)");
    assert!(any_multi_token_code_line, "no non-mermaid, non-langless design.md code fence produced a real multi-span (per-token) tokenization — 10.7 unproven on real corpus (would also fail if syntax highlighting silently fell back to plain-text)");
}

#[test]
fn feature_smoke_fixture_exercises_checkbox_link_image_footnote_and_tab() {
    // 실제 design.md 코퍼스에는 체크박스·링크·이미지·각주가 전혀 없다(실측,
    // 이 파일 상단 주석 참조) — 그래서 이 항목들은 전용 스모크 픽스처로 확인한다.
    let rendered = markdown::render(FEATURE_SMOKE_MD, WIDTH);
    let all = rendered.plain.join("\n");

    assert!(all.contains('\u{2713}'), "checkbox done glyph (✓) missing: {all}");
    assert!(all.contains('□'), "checkbox todo glyph missing: {all}");
    assert!(all.contains("(https://example.com)"), "link URL missing: {all}");
    assert!(all.contains("\u{1F5BC} 그림 (./x.png)"), "image label missing: {all}");
    assert_eq!(rendered.footnotes.len(), 1, "footnote not collected: {all}");
    assert!(all.contains("[1] 각주 본문"), "footnote body missing at document end: {all}");
    assert!(all.contains("    println"), "tab was not expanded to 4 spaces in code fence: {all}");
}
