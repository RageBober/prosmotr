//! xlsx / xls / xlsb / ods → форма `Table`.
//!
//! Один крейт calamine закрывает и современный Excel, и мёртвый xls,
//! и таблицы LibreOffice. Интерфейс при этом не меняется: та же форма
//! `Table`, что и у csv, со своей сортировкой и фильтром.

use crate::Doc;
use calamine::{open_workbook_auto, Data, Reader};
use std::path::Path;

pub fn read(path: &Path, max_rows: usize) -> Doc {
    let ext = crate::doc::ext_of(path);

    let mut wb = match open_workbook_auto(path) {
        Ok(wb) => wb,
        Err(e) => {
            return Doc::Unsupported {
                ext,
                message: format!("Не удалось открыть книгу: {e}"),
            }
        }
    };

    let names = wb.sheet_names().to_vec();
    let Some(first) = names.first().cloned() else {
        return Doc::Unsupported {
            ext,
            message: "В книге нет ни одного листа".into(),
        };
    };

    let range = match wb.worksheet_range(&first) {
        Ok(r) => r,
        Err(e) => {
            return Doc::Unsupported {
                ext,
                message: format!("Лист «{first}» не читается: {e}"),
            }
        }
    };

    let mut it = range.rows();
    let columns: Vec<String> = match it.next() {
        Some(r) => r.iter().map(cell).collect(),
        None => {
            return Doc::Table {
                columns: vec![],
                rows: vec![],
                total_rows: 0,
                truncated: false,
                delimiter: sheet_label(&first, &names),
            }
        }
    };

    let total = range.height().saturating_sub(1);
    let rows: Vec<Vec<String>> = it
        .take(max_rows)
        .map(|r| {
            let mut v: Vec<String> = r.iter().map(cell).collect();
            v.resize(columns.len().max(v.len()), String::new());
            v
        })
        .collect();

    Doc::Table {
        truncated: total > rows.len(),
        total_rows: total,
        delimiter: sheet_label(&first, &names),
        columns,
        rows,
    }
}

/// В шапке вместо разделителя показываем имя листа: для книги это полезнее.
fn sheet_label(first: &str, all: &[String]) -> String {
    if all.len() > 1 {
        format!("лист «{first}» из {}", all.len())
    } else {
        format!("лист «{first}»")
    }
}

/// Числа без хвоста `.0`, даты — как их отдаёт calamine, пустые ячейки — пусто.
fn cell(d: &Data) -> String {
    match d {
        Data::Empty => String::new(),
        Data::String(s) => s.clone(),
        Data::Float(f) => {
            if (f.fract()).abs() < f64::EPSILON && f.abs() < 1e15 {
                format!("{}", *f as i64)
            } else {
                format!("{f}")
            }
        }
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => (if *b { "да" } else { "нет" }).to_string(),
        Data::Error(e) => format!("#{e:?}"),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_lose_the_trailing_zero() {
        assert_eq!(cell(&Data::Float(475.0)), "475");
        assert_eq!(cell(&Data::Float(2.5)), "2.5");
    }

    #[test]
    fn empty_stays_empty() {
        assert_eq!(cell(&Data::Empty), "");
    }

    #[test]
    fn bools_are_russian() {
        assert_eq!(cell(&Data::Bool(true)), "да");
    }

    #[test]
    fn reads_real_xlsx() {
        // фикстура создаётся тестовым скриптом; без неё тест пропускается
        let p = std::env::temp_dir().join("prosmotr-fixture.xlsx");
        if !p.exists() {
            return;
        }
        match read(&p, 100) {
            Doc::Table { columns, rows, total_rows, .. } => {
                assert_eq!(columns, vec!["Файл", "Часы"]);
                assert_eq!(total_rows, 3);
                assert_eq!(rows[0], vec!["01-devops", "475"]);
            }
            other => panic!("ожидали Table, получили {other:?}"),
        }
    }
}
