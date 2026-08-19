use serde::Serialize;
use std::path::Path;

use crate::formats;

/// Пять форм, в которых документ доходит до интерфейса.
///
/// Ровно от этого списка зависит объём работы в UI. Он не растёт
/// при добавлении форматов — растёт только число парсеров.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Doc {
    /// Простой текст: логи, конфиги, исходный код.
    Text {
        text: String,
        /// Подсказка для подсветки синтаксиса: "rust", "python", "yaml"…
        lang: Option<String>,
        truncated: bool,
    },
    /// Готовая разметка: markdown, а позже docx, odt, epub.
    Html { html: String },
    /// Таблица: csv, tsv, а позже xlsx, ods, parquet.
    Table {
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
        total_rows: usize,
        truncated: bool,
        /// Каким разделителем оказался файл — показываем в шапке.
        delimiter: String,
    },
    /// Дерево: json, yaml, toml, xml.
    Tree { json: serde_json::Value },
    /// Страницы: pdf. Байты забирает отдельная команда, здесь только тип.
    Page { note: String },
    /// Картинка. Байты тоже приходят отдельно — незачем гонять их через json.
    Image { mime: String },
    /// Формат опознан, но пока не поддержан — честно говорим об этом.
    Unsupported { ext: String, message: String },
}

/// Ограничения, чтобы окно не вешалось на файле в 2 ГБ.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Максимум байт, читаемых из текстового файла.
    pub max_text_bytes: usize,
    /// Максимум строк таблицы за один раз.
    pub max_rows: usize,
    /// Файлы больше этого размера не открываем вовсе.
    pub max_file_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_text_bytes: 2 * 1024 * 1024,
            max_rows: 5_000,
            max_file_bytes: 512 * 1024 * 1024,
        }
    }
}

/// Расширение в нижнем регистре, без точки.
pub fn ext_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Тип картинки по расширению. `None` — значит это не картинка.
pub fn mime_of(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "svg" => "image/svg+xml",
        _ => return None,
    })
}

/// Категория файла для списка слева.
pub fn category(ext: &str) -> &'static str {
    match ext {
        "md" | "markdown" | "mdown" | "mkd" => "doc",
        "pdf" | "docx" | "odt" | "rtf" | "epub" => "doc",
        "csv" | "tsv" | "xlsx" | "xlsm" | "xls" | "xlsb" | "ods" | "parquet" => "table",
        "json" | "yaml" | "yml" | "toml" | "xml" | "ini" | "conf" => "data",
        "txt" | "log" | "" => "text",
        _ if mime_of(ext).is_some() => "image",
        _ if formats::code::lang_for(ext).is_some() => "code",
        _ => "other",
    }
}

/// Открыть файл и вернуть его в одной из пяти форм.
///
/// Диспетчер намеренно плоский: одно место, где расширение
/// превращается в парсер. Новый формат — одна строка здесь.
pub fn open_document(path: &Path, limits: Limits) -> std::io::Result<Doc> {
    let meta = std::fs::metadata(path)?;
    if meta.len() > limits.max_file_bytes {
        return Ok(Doc::Unsupported {
            ext: ext_of(path),
            message: format!(
                "Файл слишком большой: {:.1} МБ. Предел — {} МБ.",
                meta.len() as f64 / 1_048_576.0,
                limits.max_file_bytes / 1_048_576
            ),
        });
    }

    let ext = ext_of(path);
    match ext.as_str() {
        "md" | "markdown" | "mdown" | "mkd" => {
            let (s, _) = crate::text::read_text(path, limits.max_text_bytes)?;
            Ok(formats::markdown::render(&s))
        }
        "csv" | "tsv" => {
            let (s, _) = crate::text::read_text(path, usize::MAX)?;
            Ok(formats::table::from_delimited(&s, &ext, limits.max_rows))
        }
        "json" => {
            let (s, _) = crate::text::read_text(path, limits.max_text_bytes)?;
            Ok(formats::tree::from_json(&s))
        }
        "yaml" | "yml" => {
            let (s, _) = crate::text::read_text(path, limits.max_text_bytes)?;
            Ok(formats::data::from_yaml(&s))
        }
        "toml" => {
            let (s, _) = crate::text::read_text(path, limits.max_text_bytes)?;
            Ok(formats::data::from_toml(&s))
        }
        "xml" => {
            let (s, _) = crate::text::read_text(path, limits.max_text_bytes)?;
            Ok(formats::data::from_xml(&s))
        }
        "pdf" => Ok(Doc::Page { note: String::new() }),
        "xlsx" | "xlsm" | "xls" | "xlsb" | "ods" => Ok(formats::excel::read(path, limits.max_rows)),
        "docx" | "odt" => Ok(formats::word::read(path)),
        _ if mime_of(&ext).is_some() => Ok(Doc::Image {
            mime: mime_of(&ext).unwrap().to_string(),
        }),
        _ => {
            let (s, truncated) = crate::text::read_text(path, limits.max_text_bytes)?;
            if crate::text::looks_binary(s.as_bytes()) {
                return Ok(Doc::Unsupported {
                    ext,
                    message: "Похоже на двоичный файл — показывать нечего".into(),
                });
            }
            Ok(Doc::Text {
                text: s,
                lang: formats::code::lang_for(&ext).map(|s| s.to_string()),
                truncated,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp(name: &str, body: &[u8]) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(name);
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(body).unwrap();
        p
    }

    #[test]
    fn markdown_goes_to_html() {
        let p = tmp("t1.md", b"# Cabecera\n\ntext");
        match open_document(&p, Limits::default()).unwrap() {
            Doc::Html { html } => assert!(html.contains("<h1>")),
            other => panic!("ожидали Html, получили {other:?}"),
        }
    }

    #[test]
    fn csv_goes_to_table() {
        let p = tmp("t2.csv", "a,b\n1,2\n3,4\n".as_bytes());
        match open_document(&p, Limits::default()).unwrap() {
            Doc::Table { columns, rows, total_rows, .. } => {
                assert_eq!(columns, vec!["a", "b"]);
                assert_eq!(total_rows, 2);
                assert_eq!(rows[1], vec!["3", "4"]);
            }
            other => panic!("ожидали Table, получили {other:?}"),
        }
    }

    #[test]
    fn json_goes_to_tree() {
        let p = tmp("t3.json", br#"{"a":[1,2]}"#);
        assert!(matches!(
            open_document(&p, Limits::default()).unwrap(),
            Doc::Tree { .. }
        ));
    }

    #[test]
    fn unknown_text_goes_to_text() {
        let p = tmp("t4.log", "2026-08-19 запуск\n".as_bytes());
        assert!(matches!(
            open_document(&p, Limits::default()).unwrap(),
            Doc::Text { .. }
        ));
    }

    #[test]
    fn yaml_and_toml_go_to_tree() {
        let y = tmp("t7.yaml", "часы: 475\n".as_bytes());
        let t = tmp("t8.toml", "hours = 475\n".as_bytes());
        assert!(matches!(open_document(&y, Limits::default()).unwrap(), Doc::Tree { .. }));
        assert!(matches!(open_document(&t, Limits::default()).unwrap(), Doc::Tree { .. }));
    }

    #[test]
    fn images_go_to_image_form() {
        let p = tmp("t9.png", &[0x89, 0x50, 0x4E, 0x47]);
        match open_document(&p, Limits::default()).unwrap() {
            Doc::Image { mime } => assert_eq!(mime, "image/png"),
            other => panic!("ожидали Image, получили {other:?}"),
        }
    }

    #[test]
    fn pdf_goes_to_page_form() {
        let p = tmp("t10.pdf", b"%PDF-1.7");
        assert!(matches!(open_document(&p, Limits::default()).unwrap(), Doc::Page { .. }));
    }

    #[test]
    fn binary_is_refused_politely() {
        let p = tmp("t5.bin", &[0u8, 1, 2, 3, 0, 9]);
        assert!(matches!(
            open_document(&p, Limits::default()).unwrap(),
            Doc::Unsupported { .. }
        ));
    }

    #[test]
    fn oversized_file_is_refused() {
        let p = tmp("t6.txt", b"hello");
        let limits = Limits { max_file_bytes: 1, ..Default::default() };
        match open_document(&p, limits).unwrap() {
            Doc::Unsupported { message, .. } => assert!(message.contains("большой")),
            other => panic!("ожидали отказ, получили {other:?}"),
        }
    }
}
