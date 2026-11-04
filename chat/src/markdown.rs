//! Markdown of the chat replies as HTML.
//!
//! The replies come from `claude` and can contain any text, so the HTML is
//! safe for the page: raw HTML shows as text, links keep only the schemes
//! `http`, `https` and `mailto`, and images show as links.

use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, TagEnd, html};

/// Escapes `text` for HTML text and attribute values.
pub fn escape(text: &str) -> String {
    let mut s = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => s.push_str("&amp;"),
            '<' => s.push_str("&lt;"),
            '>' => s.push_str("&gt;"),
            '"' => s.push_str("&quot;"),
            '\'' => s.push_str("&#39;"),
            _ => s.push(c),
        }
    }
    s
}

/// True if a link to `url` may open: an absolute URL with the scheme
/// `http`, `https` or `mailto`.
fn safe_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    ["http://", "https://", "mailto:"]
        .iter()
        .any(|p| lower.starts_with(p))
}

/// The HTML of the Markdown text `text`.
pub fn to_html(text: &str) -> String {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM;
    // An image shows as a link with its alternative text.
    let events = Parser::new_ext(text, options).map(|ev| match ev {
        Event::Html(h) | Event::InlineHtml(h) => Event::Text(h),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        })
        | Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => {
            let dest_url = if safe_url(&dest_url) {
                dest_url
            } else {
                CowStr::Borrowed("")
            };
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            })
        }
        Event::End(TagEnd::Image) => Event::End(TagEnd::Link),
        other => other,
    });
    let mut out = String::new();
    html::push_html(&mut out, events);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_html_shows_as_text() {
        let h = to_html("Hi <script>alert(1)</script> <b>x</b>");
        assert!(!h.contains("<script>") && !h.contains("<b>"));
        assert!(h.contains("&lt;script&gt;"));
        let block = to_html("<div onclick=\"x\">\n\ntext");
        assert!(!block.contains("<div"));
    }

    #[test]
    fn only_safe_links_keep_their_target() {
        let h = to_html("[a](https://example.com) [b](javascript:alert(1)) [c](file:///etc)");
        assert!(h.contains(r#"href="https://example.com""#));
        assert!(!h.contains("javascript:") && !h.contains("file:"));
    }

    #[test]
    fn images_become_links() {
        let h = to_html("![logo](https://example.com/a.png)");
        assert!(!h.contains("<img"));
        assert!(h.contains(r#"<a href="https://example.com/a.png">logo</a>"#));
    }

    #[test]
    fn tables_and_code_render() {
        let h = to_html("| a | b |\n|---|---|\n| 1 | 2 |\n\n```rust\nlet x = 1 < 2;\n```\n");
        assert!(h.contains("<table>") && h.contains("<td>2</td>"));
        assert!(h.contains(r#"<code class="language-rust">let x = 1 &lt; 2;"#));
    }

    #[test]
    fn escape_covers_attributes() {
        assert_eq!(
            escape(r#"<a href="x">'&"#),
            "&lt;a href=&quot;x&quot;&gt;&#39;&amp;"
        );
    }
}
