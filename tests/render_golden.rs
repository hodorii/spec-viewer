use spec_viewer::markdown;

const SOURCE: &str = include_str!("fixtures/golden/source.md");
const WIDTHS: [u16; 3] = [40, 80, 120];

fn render_plain(width: u16) -> String {
    markdown::render(SOURCE, width).plain.join("\n")
}

fn snapshot_path(width: u16) -> String {
    format!("fixtures/golden/w{}.txt", width)
}

#[test]
#[ignore]
fn generate_golden_snapshots() {
    for width in WIDTHS {
        let rendered = render_plain(width);
        let path = format!("{}/tests/{}", env!("CARGO_MANIFEST_DIR"), snapshot_path(width));
        std::fs::write(&path, rendered).unwrap_or_else(|e| panic!("failed to write {path}: {e}"));
    }
}

macro_rules! golden_width_test {
    ($name:ident, $width:expr, $file:expr) => {
        #[test]
        fn $name() {
            let expected = include_str!(concat!("fixtures/golden/", $file));
            let actual = render_plain($width);
            assert_eq!(
                actual, expected,
                "render output at width {} no longer matches the committed golden snapshot {}",
                $width, $file
            );
        }
    };
}

golden_width_test!(test_golden_width_40, 40, "w40.txt");
golden_width_test!(test_golden_width_80, 80, "w80.txt");
golden_width_test!(test_golden_width_120, 120, "w120.txt");

#[test]
fn test_width_80_contains_real_mermaid_rendering() {
    let text = render_plain(80);
    assert!(text.contains('│'), "expected box-drawing vertical lines in width-80 render:\n{text}");
    assert!(
        text.contains('►') || text.contains('▼'),
        "expected at least one real mermaid arrow head in width-80 render:\n{text}"
    );
}

#[test]
fn test_width_120_contains_real_mermaid_rendering() {
    let text = render_plain(120);
    assert!(text.contains('│'), "expected box-drawing vertical lines in width-120 render:\n{text}");
    assert!(
        text.contains('►') || text.contains('▼'),
        "expected at least one real mermaid arrow head in width-120 render:\n{text}"
    );
}

#[test]
fn test_no_panic_at_all_widths_with_cjk_content() {
    for width in WIDTHS {
        let text = render_plain(width);
        assert!(text.contains("한글") || text.contains("최상위"), "expected CJK content to survive render at width {width}");
    }
}
