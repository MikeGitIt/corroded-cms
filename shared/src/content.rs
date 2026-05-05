use pulldown_cmark::{Options, Parser, html};

pub fn render_markdown(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(markdown, options);
    let mut raw_html = String::new();
    html::push_html(&mut raw_html, parser);

    sanitize_html(&raw_html)
}

pub fn sanitize_html(html: &str) -> String {
    ammonia::Builder::default().clean(html).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_basic_markdown() {
        let html = render_markdown("# Hello\n\nThis is **strong**.");

        assert!(html.contains("<h1>Hello</h1>"));
        assert!(html.contains("<strong>strong</strong>"));
    }

    #[test]
    fn removes_unsafe_scripts() {
        let html = render_markdown("<script>alert('xss')</script>\n\n[bad](javascript:alert(1))");

        assert!(!html.contains("<script>"));
        assert!(!html.contains("javascript:"));
    }

    #[test]
    fn preserves_safe_images() {
        let html = render_markdown("![Alt text](/uploads/2026/05/image.png)");

        assert!(html.contains(r#"<img src="/uploads/2026/05/image.png" alt="Alt text""#));
    }
}
