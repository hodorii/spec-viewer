use std::time::Instant;

fn generate_complex_markdown() -> String {
    let mut md = String::new();
    let mut lines = 0;
    
    let elements = [
        "Heading",
        "Paragraph",
        "List",
        "Table",
        "CodeFence",
        "Mermaid",
    ];

    while lines < 500 {
        match elements[lines % elements.len()] {
            "Heading" => {
                md.push_str("# Heading Level 1\n\n");
                lines += 2;
            }
            "Paragraph" => {
                md.push_str("This is a paragraph of text that contains some detailed information about the spec viewer performance testing. It needs to be long enough to contribute to the line count.\n\n");
                lines += 2;
            }
            "List" => {
                md.push_str("- Item 1\n- Item 2\n- Item 3\n- Item 4\n\n");
                lines += 5;
            }
            "Table" => {
                md.push_str("| Col 1 | Col 2 |\n|---|---|\n| Val 1 | Val 2 |\n| Val 3 | Val 4 |\n\n");
                lines += 4;
            }
            "CodeFence" => {
                md.push_str("```rust\nfn main() {\n    println!(\"Hello World\");\n}\n```\n\n");
                lines += 5;
            }
            "Mermaid" => {
                md.push_str("```mermaid\ngraph TD\n    A --> B\n    B --> C\n    C --> A\n```\n\n");
                lines += 6;
            }
            _ => unreachable!(),
        }
    }
    md
}

#[test]
fn test_render_performance() {
    let src = generate_complex_markdown();
    
    // Warm-up
    let _ = spec_viewer::markdown::render(&src, 80);
    
    let mut times = Vec::new();
    for _ in 0..3 {
        let start = Instant::now();
        let _ = spec_viewer::markdown::render(&src, 80);
        times.push(start.elapsed());
    }
    
    let min_time = times.iter().min().unwrap();
    let ms = min_time.as_millis();
    println!("Render time: {}ms", ms);
    
    assert!(ms < 50, "Render performance regression: minimum time was {}ms, expected < 50ms", ms);
}
