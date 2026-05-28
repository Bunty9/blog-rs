use pulldown_cmark::{html, Options, Parser};

/// Render CommonMark to HTML with sensible options. No shortcode handling yet.
pub fn to_html(src: &str) -> String {
    let opts = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(src, opts);
    let mut out = String::with_capacity(src.len());
    html::push_html(&mut out, parser);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_basic_markdown() {
        let html = to_html("# Hello\n\n**bold**");
        assert!(html.contains("<h1>Hello</h1>"));
        assert!(html.contains("<strong>bold</strong>"));
    }

    #[test]
    fn renders_tables() {
        let md = "| a | b |\n|---|---|\n| 1 | 2 |";
        let html = to_html(md);
        assert!(html.contains("<table>"));
        assert!(html.contains("<th>a</th>"));
    }

    #[test]
    fn passes_through_inline_html() {
        let html = to_html("<div class=\"x\">y</div>");
        assert!(html.contains("<div class=\"x\">y</div>"));
    }
}
