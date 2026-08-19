//! csv / tsv → форма `Table`.
//!
//! Здесь же живёт определение разделителя: файлы с расширением `.csv`
//! в русской локали регулярно оказываются с точкой с запятой, потому что
//! Excel так сохраняет при запятой в роли десятичного знака.

use crate::Doc;

const CANDIDATES: [u8; 4] = [b',', b';', b'\t', b'|'];

/// Угадать разделитель по первой непустой строке.
///
/// Берём тот символ, который встречается чаще, но только вне кавычек —
/// иначе `"Иванов, Иван";30` даст неверный ответ.
pub fn sniff_delimiter(sample: &str, ext: &str) -> u8 {
    if ext.eq_ignore_ascii_case("tsv") {
        return b'\t';
    }
    let line = sample.lines().find(|l| !l.trim().is_empty()).unwrap_or("");

    let mut best = b',';
    let mut best_count = 0usize;
    for cand in CANDIDATES {
        let mut in_quotes = false;
        let mut count = 0usize;
        for b in line.bytes() {
            match b {
                b'"' => in_quotes = !in_quotes,
                x if x == cand && !in_quotes => count += 1,
                _ => {}
            }
        }
        if count > best_count {
            best_count = count;
            best = cand;
        }
    }
    best
}

pub fn from_delimited(src: &str, ext: &str, max_rows: usize) -> Doc {
    let delim = sniff_delimiter(src, ext);

    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delim)
        .flexible(true) // строки разной длины не должны ронять просмотр
        .from_reader(src.as_bytes());

    let columns: Vec<String> = match rdr.headers() {
        Ok(h) => h.iter().map(|s| s.to_string()).collect(),
        Err(e) => {
            return Doc::Unsupported {
                ext: ext.to_string(),
                message: format!("Не удалось разобрать таблицу: {e}"),
            }
        }
    };

    let width = columns.len();
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut total = 0usize;

    for rec in rdr.records() {
        let Ok(rec) = rec else { continue };
        total += 1;
        if rows.len() < max_rows {
            let mut row: Vec<String> = rec.iter().map(|s| s.to_string()).collect();
            row.resize(width.max(row.len()), String::new());
            rows.push(row);
        }
    }

    Doc::Table {
        columns,
        rows,
        total_rows: total,
        truncated: total > max_rows,
        delimiter: match delim {
            b'\t' => "TAB".to_string(),
            d => (d as char).to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(src: &str, ext: &str) -> (Vec<String>, Vec<Vec<String>>, usize, String) {
        match from_delimited(src, ext, 1000) {
            Doc::Table { columns, rows, total_rows, delimiter, .. } => {
                (columns, rows, total_rows, delimiter)
            }
            other => panic!("ожидали Table, получили {other:?}"),
        }
    }

    #[test]
    fn detects_semicolon() {
        let (cols, rows, _, d) = table("имя;возраст\nИван;30\n", "csv");
        assert_eq!(d, ";");
        assert_eq!(cols, vec!["имя", "возраст"]);
        assert_eq!(rows[0], vec!["Иван", "30"]);
    }

    #[test]
    fn ignores_delimiter_inside_quotes() {
        // запятых внутри кавычек больше, чем точек с запятой снаружи
        let d = sniff_delimiter("\"Иванов, Иван, Иванович\";30", "csv");
        assert_eq!(d as char, ';');
    }

    #[test]
    fn tsv_by_extension() {
        let (cols, _, _, d) = table("a\tb\n1\t2\n", "tsv");
        assert_eq!(d, "TAB");
        assert_eq!(cols.len(), 2);
    }

    #[test]
    fn counts_all_rows_but_returns_limit() {
        let mut src = String::from("a\n");
        for i in 0..50 {
            src.push_str(&format!("{i}\n"));
        }
        match from_delimited(&src, "csv", 10) {
            Doc::Table { rows, total_rows, truncated, .. } => {
                assert_eq!(rows.len(), 10);
                assert_eq!(total_rows, 50);
                assert!(truncated);
            }
            other => panic!("ожидали Table, получили {other:?}"),
        }
    }

    #[test]
    fn survives_ragged_rows() {
        let (_, rows, total, _) = table("a,b,c\n1,2\n1,2,3,4\n", "csv");
        assert_eq!(total, 2);
        assert_eq!(rows[0].len(), 3); // короткая строка дополнена пустыми
    }
}
