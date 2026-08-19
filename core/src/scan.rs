//! Обход папки: что показать в списке слева.

use serde::Serialize;
use std::path::Path;
use walkdir::WalkDir;

use crate::doc::{category, ext_of};

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FileEntry {
    /// Полный путь — с ним потом придёт запрос на открытие.
    pub path: String,
    /// Путь относительно корня — его показываем.
    pub rel: String,
    pub name: String,
    pub dir: String,
    pub ext: String,
    /// doc | table | data | text | code | image
    pub category: String,
    pub size: u64,
}

/// Папки, в которые заходить бессмысленно: там тысячи файлов,
/// и ни один человек не открывает их глазами.
const SKIP_DIRS: [&str; 8] = [
    "node_modules",
    "target",
    "__pycache__",
    "venv",
    ".venv",
    "dist",
    "build",
    ".cache",
];

const MAX_DEPTH: usize = 8;
const MAX_ENTRIES: usize = 20_000;

pub fn scan_folder(root: &Path) -> Vec<FileEntry> {
    let mut out = Vec::new();

    let walker = WalkDir::new(root)
        .max_depth(MAX_DEPTH)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            if name.starts_with('.') && e.depth() > 0 {
                return false;
            }
            if e.file_type().is_dir() && SKIP_DIRS.contains(&name.as_ref()) {
                return false;
            }
            true
        });

    for entry in walker.flatten() {
        if !entry.file_type().is_file() || out.len() >= MAX_ENTRIES {
            continue;
        }
        let path = entry.path();
        let ext = ext_of(path);
        let cat = category(&ext);
        if cat == "other" {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let dir = match rel.rfind('/') {
            Some(i) => rel[..i].to_string(),
            None => String::new(),
        };
        out.push(FileEntry {
            path: path.to_string_lossy().into_owned(),
            name: entry.file_name().to_string_lossy().into_owned(),
            rel,
            dir,
            ext,
            category: cat.to_string(),
            size: entry.metadata().map(|m| m.len()).unwrap_or(0),
        });
    }

    // Сначала по папке, потом по имени: иначе `core/src/doc.rs` встанет
    // между `core/src/formats/*` и `core/src/lib.rs`, и заголовки папок
    // в списке начнут повторяться.
    out.sort_by(|a, b| natural_cmp(&a.dir, &b.dir).then_with(|| natural_cmp(&a.name, &b.name)));
    out
}

/// Сравнение с учётом чисел: `02-file` идёт перед `10-file`,
/// а не после, как при обычном лексикографическом порядке.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());

    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                if x.is_ascii_digit() && y.is_ascii_digit() {
                    let na = take_number(&mut ai);
                    let nb = take_number(&mut bi);
                    match na.cmp(&nb) {
                        Ordering::Equal => continue,
                        ord => return ord,
                    }
                } else {
                    let (lx, ly) = (lower(x), lower(y));
                    if lx != ly {
                        return lx.cmp(&ly);
                    }
                    ai.next();
                    bi.next();
                }
            }
        }
    }
}

fn lower(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

fn take_number(it: &mut std::iter::Peekable<std::str::Chars>) -> u64 {
    let mut n: u64 = 0;
    while let Some(c) = it.peek().copied() {
        if !c.is_ascii_digit() {
            break;
        }
        n = n.saturating_mul(10).saturating_add(c as u64 - '0' as u64);
        it.next();
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    #[test]
    fn numbers_sort_naturally() {
        let mut v = vec!["10-b.md", "2-a.md", "1-c.md"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, vec!["1-c.md", "2-a.md", "10-b.md"]);
    }

    #[test]
    fn zero_padded_equals_plain() {
        assert_eq!(natural_cmp("02-a", "2-a"), Ordering::Equal);
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(natural_cmp("План.md", "план.md"), Ordering::Equal);
    }

    #[test]
    fn scans_and_filters() {
        let root = std::env::temp_dir().join("prosmotr-scan-test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("sub/deep")).unwrap();
        std::fs::create_dir_all(root.join("node_modules")).unwrap();
        std::fs::write(root.join("10-b.md"), "b").unwrap();
        std::fs::write(root.join("2-a.md"), "a").unwrap();
        std::fs::write(root.join("sub/data.csv"), "x,y").unwrap();
        std::fs::write(root.join("node_modules/junk.md"), "no").unwrap();
        std::fs::write(root.join(".hidden.md"), "no").unwrap();
        std::fs::write(root.join("photo.bin"), [0u8, 1]).unwrap();

        std::fs::write(root.join("sub/zzz.md"), "z").unwrap();
        std::fs::write(root.join("sub/deep/inner.md"), "i").unwrap();

        let found = scan_folder(&root);
        let names: Vec<&str> = found.iter().map(|f| f.rel.as_str()).collect();
        // файлы одной папки идут подряд, а не вперемешку с вложенными
        assert_eq!(
            names,
            vec![
                "2-a.md",
                "10-b.md",
                "sub/data.csv",
                "sub/zzz.md",
                "sub/deep/inner.md"
            ]
        );
        assert_eq!(found[2].category, "table");
        assert_eq!(found[2].dir, "sub");
    }
}
