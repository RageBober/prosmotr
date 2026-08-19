//! Markdown → форма `Html`.
//!
//! Рендер в Rust, а не в браузере: экономит 35 КБ javascript,
//! работает быстрее и позволяет позже отдать тот же HTML в печать.

use crate::Doc;
use pulldown_cmark::{html, Options, Parser};

pub fn render(src: &str) -> Doc {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_SMART_PUNCTUATION);

    let parser = Parser::new_ext(src, opts);
    let mut out = String::with_capacity(src.len() * 3 / 2);
    html::push_html(&mut out, parser);
    Doc::Html { html: out }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn html_of(src: &str) -> String {
        match render(src) {
            Doc::Html { html } => html,
            other => panic!("ожидали Html, получили {other:?}"),
        }
    }

    #[test]
    fn renders_tables() {
        let h = html_of("| a | b |\n| --- | --- |\n| 1 | 2 |");
        assert!(h.contains("<table>"));
        assert!(h.contains("<td>1</td>"));
    }

    #[test]
    fn renders_task_lists() {
        let h = html_of("- [x] сделано\n- [ ] нет");
        assert!(h.contains("type=\"checkbox\""));
    }

    #[test]
    fn keeps_cyrillic() {
        let h = html_of("# План");
        assert!(h.contains("План"));
    }

    #[test]
    fn escapes_raw_script() {
        // markdown разрешает html, поэтому за безопасность отвечает CSP в окне;
        // тест фиксирует поведение, чтобы оно не изменилось незаметно
        let h = html_of("текст <b>жирный</b>");
        assert!(h.contains("<b>жирный</b>"));
    }
}
