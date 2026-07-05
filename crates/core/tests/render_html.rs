//! Golden-ish assertions for the HTML renderer: every markdown construct maps
//! to the expected HTML shape. Uses invariants rather than byte-exact goldens
//! so the tests stay readable and resilient to incidental whitespace.

use papery_core::render_html::{render_html, RenderOptions};

fn frag(md: &str) -> String {
    render_html(md, &RenderOptions::preview()).unwrap()
}

#[test]
fn headings_get_ids_and_anchors() {
    let h = frag("# Hello World\n\n## Section\n");
    assert!(h.contains(r#"<h1 id="hello-world">"#));
    assert!(h.contains(r##"href="#hello-world""##));
    assert!(h.contains(r#"<h2 id="section">"#));
}

#[test]
fn inline_formatting() {
    let h = frag("**b** *i* ~~s~~ `c` [l](https://x.com)\n");
    assert!(h.contains("<strong>b</strong>"));
    assert!(h.contains("<em>i</em>"));
    assert!(h.contains("<del>s</del>"));
    assert!(h.contains("<code>c</code>"));
    assert!(h.contains(r#"<a href="https://x.com">l</a>"#));
}

#[test]
fn html_special_chars_are_escaped() {
    let h = frag("a < b & c > d\n");
    assert!(h.contains("a &lt; b &amp; c &gt; d"));
    assert!(!h.contains("a < b"));
}

#[test]
fn task_list_renders_checkboxes() {
    let h = frag("- [x] done\n- [ ] todo\n");
    assert!(h.contains(r#"class="task-list-item""#));
    assert!(h.contains(r#"type="checkbox" disabled checked"#));
    assert!(h.contains(r#"type="checkbox" disabled>"#));
}

#[test]
fn ordered_list_start_attribute() {
    let h = frag("3. three\n4. four\n");
    assert!(h.contains(r#"<ol start="3">"#));
}

#[test]
fn table_alignment_styles() {
    let md = "| L | C | R |\n|:--|:-:|--:|\n| a | b | c |\n";
    let h = frag(md);
    assert!(h.contains("<table>"));
    assert!(h.contains(r#"<th style="text-align:left">L</th>"#));
    assert!(h.contains(r#"<th style="text-align:center">C</th>"#));
    assert!(h.contains(r#"<th style="text-align:right">R</th>"#));
    assert!(h.contains("<thead>") && h.contains("<tbody>"));
}

#[test]
fn code_block_is_highlighted_with_classes() {
    let h = frag("```rust\nfn main() {}\n```\n");
    assert!(h.contains(r#"<pre class="mdcode">"#));
    assert!(h.contains(r#"class="language-rust""#));
    // syntect class spans present
    assert!(h.contains("<span"));
}

#[test]
fn math_uses_katex_delimiters_when_enabled() {
    let h = frag("Inline $a^2$ and\n\n$$E=mc^2$$\n");
    assert!(h.contains(r#"<span class="math math-inline">\(a^2\)</span>"#));
    assert!(h.contains(r#"<span class="math math-display">\[E=mc^2\]</span>"#));
}

#[test]
fn blockquote_and_hr() {
    let h = frag("> quote\n\n---\n");
    assert!(h.contains("<blockquote>"));
    assert!(h.contains("<hr />"));
}

#[test]
fn standalone_is_self_contained() {
    let opts = RenderOptions::standalone();
    let html = render_html("# Title\n\ntext\n", &opts).unwrap();
    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("<style>"));
    assert!(html.contains(".papery")); // embedded theme
    assert!(html.contains("<title>Title</title>"));
    // No external resource references (self-contained requirement).
    assert!(!html.contains("http://"));
    assert!(!html.contains("https://"));
}

#[test]
fn front_matter_is_stripped_from_output() {
    let h = frag("---\ntitle: Secret\n---\n\n# Visible\n");
    assert!(!h.contains("Secret"));
    assert!(h.contains("Visible"));
}

#[test]
fn dangerous_link_schemes_are_neutralized() {
    let h = frag("[x](javascript:alert(1)) [y](  JavaScript:alert(2)) [z](data:text/html,<b>)\n");
    assert!(!h.to_lowercase().contains("javascript:"));
    assert!(!h.contains("data:text/html"));
    // Neutralized links point at a harmless anchor.
    assert!(h.contains(r##"href="#""##));
}

#[test]
fn safe_link_schemes_pass_through() {
    let h = frag("[a](https://example.com) [b](mailto:x@y.com) [c](./rel.md#frag)\n");
    assert!(h.contains(r#"href="https://example.com""#));
    assert!(h.contains(r#"href="mailto:x@y.com""#));
    assert!(h.contains(r##"href="./rel.md#frag""##));
}

#[test]
fn raw_html_is_escaped_not_executed() {
    let h = frag("before <script>alert(1)</script> after\n\n<div onclick=\"x\">block</div>\n");
    assert!(!h.contains("<script>"));
    assert!(!h.contains("<div onclick"));
    assert!(h.contains("&lt;script&gt;"));
}
