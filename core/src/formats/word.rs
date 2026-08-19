//! docx / odt → форма `Html`.
//!
//! Оба формата — это zip с xml внутри, поэтому обходимся без тяжёлых
//! конвертеров: `zip` достаёт нужный файл, `quick-xml` идёт по событиям.
//! Берём то, что вообще имеет смысл в просмотрщике: заголовки, абзацы,
//! жирный и курсив, списки и таблицы. Колонтитулы, поля и стили — нет.

use crate::Doc;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::io::Read;
use std::path::Path;

pub fn read(path: &Path) -> Doc {
    let ext = crate::doc::ext_of(path);
    let is_docx = ext == "docx";
    let inner = if is_docx { "word/document.xml" } else { "content.xml" };

    let xml = match unzip(path, inner) {
        Ok(x) => x,
        Err(e) => {
            return Doc::Unsupported {
                ext,
                message: format!("Не удалось распаковать: {e}"),
            }
        }
    };

    let html = if is_docx { docx(&xml) } else { odt(&xml) };
    if html.trim().is_empty() {
        return Doc::Unsupported {
            ext,
            message: "Документ пуст или размечен непривычно".into(),
        };
    }
    Doc::Html { html }
}

fn unzip(path: &Path, inner: &str) -> Result<String, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut entry = zip.by_name(inner).map_err(|_| format!("нет {inner}"))?;
    let mut s = String::new();
    entry.read_to_string(&mut s).map_err(|e| e.to_string())?;
    Ok(s)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Значение атрибута по имени.
fn attr(e: &quick_xml::events::BytesStart, name: &[u8]) -> Option<String> {
    e.attributes().flatten().find(|a| a.key.as_ref() == name).map(|a| {
        String::from_utf8_lossy(&a.value).into_owned()
    })
}

/// Общая сборка: абзацы кладём либо в ячейку таблицы, либо на верхний уровень.
#[derive(Default)]
struct Out {
    html: String,
    cell: Option<String>,
    list_open: bool,
}

impl Out {
    fn block(&mut self, tag: &str, body: &str) {
        if body.trim().is_empty() {
            return;
        }
        if let Some(c) = self.cell.as_mut() {
            if !c.is_empty() {
                c.push_str("<br>");
            }
            c.push_str(body);
            return;
        }
        if tag == "li" {
            if !self.list_open {
                self.html.push_str("<ul>");
                self.list_open = true;
            }
            self.html.push_str(&format!("<li>{body}</li>"));
            return;
        }
        self.close_list();
        self.html.push_str(&format!("<{tag}>{body}</{tag}>"));
    }
    fn close_list(&mut self) {
        if self.list_open {
            self.html.push_str("</ul>");
            self.list_open = false;
        }
    }
    fn raw(&mut self, s: &str) {
        self.close_list();
        self.html.push_str(s);
    }
}

/// --- docx ---
pub fn docx(xml: &str) -> String {
    let mut r = Reader::from_str(xml);
    let mut o = Out::default();

    let (mut para, mut style) = (String::new(), String::new());
    let (mut bold, mut italic, mut in_text, mut numbered) = (false, false, false, false);

    loop {
        match r.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.name().as_ref() {
                b"w:p" => {
                    para.clear();
                    style.clear();
                    numbered = false;
                }
                b"w:pStyle" => style = attr(&e, b"w:val").unwrap_or_default(),
                b"w:numPr" => numbered = true,
                b"w:r" => {
                    bold = false;
                    italic = false;
                }
                b"w:b" => bold = attr(&e, b"w:val").map(|v| v != "0" && v != "false").unwrap_or(true),
                b"w:i" => italic = attr(&e, b"w:val").map(|v| v != "0" && v != "false").unwrap_or(true),
                b"w:t" => in_text = true,
                b"w:br" | b"w:cr" => para.push_str("<br>"),
                b"w:tbl" => o.raw("<table><tbody>"),
                b"w:tr" => o.raw("<tr>"),
                b"w:tc" => o.cell = Some(String::new()),
                _ => {}
            },
            Ok(Event::Text(t)) if in_text => {
                let text = esc(&t.unescape().unwrap_or_default());
                let mut piece = text;
                if italic {
                    piece = format!("<em>{piece}</em>");
                }
                if bold {
                    piece = format!("<strong>{piece}</strong>");
                }
                para.push_str(&piece);
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"w:t" => in_text = false,
                b"w:p" => {
                    // список опознаётся двумя способами: явной нумерацией (w:numPr)
                    // и стилем вида «ListBullet» — Word пользуется обоими
                    let listy = numbered || style.to_ascii_lowercase().contains("list");
                    let tag = heading_tag(&style).unwrap_or(if listy { "li" } else { "p" });
                    let body = std::mem::take(&mut para);
                    o.block(tag, &body);
                }
                b"w:tc" => {
                    let c = o.cell.take().unwrap_or_default();
                    o.raw(&format!("<td>{c}</td>"));
                }
                b"w:tr" => o.raw("</tr>"),
                b"w:tbl" => o.raw("</tbody></table>"),
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    o.close_list();
    o.html
}

fn heading_tag(style: &str) -> Option<&'static str> {
    let s = style.to_ascii_lowercase().replace(['-', ' '], "");
    let n = s.strip_prefix("heading")?.parse::<u8>().ok()?;
    Some(match n {
        1 => "h1",
        2 => "h2",
        3 => "h3",
        4 => "h4",
        5 => "h5",
        _ => "h6",
    })
}

/// --- odt ---
pub fn odt(xml: &str) -> String {
    let mut r = Reader::from_str(xml);
    let mut o = Out::default();
    let (mut para, mut level, mut in_text, mut is_head, mut is_list) = (String::new(), 1u8, false, false, false);

    loop {
        match r.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.name().as_ref() {
                b"text:h" => {
                    para.clear();
                    is_head = true;
                    level = attr(&e, b"text:outline-level")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(1);
                    in_text = true;
                }
                b"text:p" => {
                    para.clear();
                    is_head = false;
                    in_text = true;
                }
                b"text:list" => is_list = true,
                b"text:line-break" => para.push_str("<br>"),
                b"table:table" => o.raw("<table><tbody>"),
                b"table:table-row" => o.raw("<tr>"),
                b"table:table-cell" => o.cell = Some(String::new()),
                _ => {}
            },
            Ok(Event::Text(t)) if in_text => {
                para.push_str(&esc(&t.unescape().unwrap_or_default()));
            }
            Ok(Event::End(e)) => match e.name().as_ref() {
                b"text:h" | b"text:p" => {
                    in_text = false;
                    let tag = if is_head {
                        match level {
                            1 => "h1",
                            2 => "h2",
                            3 => "h3",
                            4 => "h4",
                            5 => "h5",
                            _ => "h6",
                        }
                    } else if is_list {
                        "li"
                    } else {
                        "p"
                    };
                    let body = std::mem::take(&mut para);
                    o.block(tag, &body);
                }
                b"text:list" => {
                    is_list = false;
                    o.close_list();
                }
                b"table:table-cell" => {
                    let c = o.cell.take().unwrap_or_default();
                    o.raw(&format!("<td>{c}</td>"));
                }
                b"table:table-row" => o.raw("</tr>"),
                b"table:table" => o.raw("</tbody></table>"),
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    o.close_list();
    o.html
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"<w:document xmlns:w="x"><w:body>
      <w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Заголовок</w:t></w:r></w:p>
      <w:p><w:r><w:rPr><w:b/></w:rPr><w:t>жирно</w:t></w:r><w:r><w:t> и обычно</w:t></w:r></w:p>
      <w:p><w:pPr><w:numPr/></w:pPr><w:r><w:t>пункт один</w:t></w:r></w:p>
      <w:p><w:pPr><w:numPr/></w:pPr><w:r><w:t>пункт два</w:t></w:r></w:p>
      <w:tbl><w:tr><w:tc><w:p><w:r><w:t>ячейка</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
      <w:p><w:r><w:t></w:t></w:r></w:p>
    </w:body></w:document>"#;

    #[test]
    fn docx_headings_and_bold() {
        let h = docx(DOC);
        assert!(h.contains("<h1>Заголовок</h1>"));
        assert!(h.contains("<strong>жирно</strong> и обычно"));
    }

    #[test]
    fn docx_list_items_share_one_ul() {
        let h = docx(DOC);
        assert_eq!(h.matches("<ul>").count(), 1);
        assert_eq!(h.matches("<li>").count(), 2);
    }

    #[test]
    fn docx_tables_survive() {
        let h = docx(DOC);
        assert!(h.contains("<table><tbody><tr><td>ячейка</td></tr></tbody></table>"));
    }

    #[test]
    fn docx_skips_empty_paragraphs() {
        assert!(!docx(DOC).contains("<p></p>"));
    }

    #[test]
    fn docx_escapes_html_in_text() {
        let x = r#"<w:document xmlns:w="x"><w:p><w:r><w:t>&lt;script&gt;</w:t></w:r></w:p></w:document>"#;
        let h = docx(x);
        assert!(h.contains("&lt;script&gt;"));
        assert!(!h.contains("<script>"));
    }

    #[test]
    fn odt_headings_and_tables() {
        let x = r#"<office xmlns:text="t" xmlns:table="b">
          <text:h text:outline-level="2">Раздел</text:h>
          <text:p>абзац</text:p>
          <table:table><table:table-row><table:table-cell><text:p>я</text:p></table:table-cell></table:table-row></table:table>
        </office>"#;
        let h = odt(x);
        assert!(h.contains("<h2>Раздел</h2>"));
        assert!(h.contains("<p>абзац</p>"));
        assert!(h.contains("<td>я</td>"));
    }

    #[test]
    fn reads_real_docx() {
        // настоящий файл от python-docx; без фикстуры тест пропускается
        let p = std::env::temp_dir().join("prosmotr-fixture.docx");
        if !p.exists() {
            return;
        }
        match read(&p) {
            Doc::Html { html } => {
                assert!(html.contains("<h1>Заголовок первого уровня</h1>"), "нет заголовка: {html}");
                assert!(html.contains("<strong>жирно</strong>"), "нет жирного: {html}");
                assert!(html.contains("<li>пункт один</li>"), "нет списка: {html}");
                assert!(html.contains("<td>ячейка</td>"), "нет таблицы: {html}");
            }
            other => panic!("ожидали Html, получили {other:?}"),
        }
    }

    #[test]
    fn heading_style_names() {
        assert_eq!(heading_tag("Heading1"), Some("h1"));
        assert_eq!(heading_tag("heading 3"), Some("h3"));
        assert_eq!(heading_tag("Normal"), None);
    }
}
