//! Сопоставление расширения и языка — подсказка для подсветки синтаксиса.
//!
//! Сама подсветка появится в v2; ядру достаточно вернуть имя языка,
//! чтобы интерфейс не гадал по расширению во второй раз.

pub fn lang_for(ext: &str) -> Option<&'static str> {
    Some(match ext {
        "rs" => "rust",
        "py" | "pyw" => "python",
        "js" | "mjs" | "cjs" => "javascript",
        "ts" | "tsx" => "typescript",
        "sh" | "bash" | "zsh" => "bash",
        "sql" => "sql",
        "c" | "h" => "c",
        "cpp" | "cc" | "hpp" => "cpp",
        "go" => "go",
        "java" => "java",
        "rb" => "ruby",
        "php" => "php",
        "r" => "r",
        "jl" => "julia",
        "html" | "htm" => "html",
        "css" | "scss" => "css",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "ini" | "conf" | "cfg" => "ini",
        "xml" | "svg" => "xml",
        "diff" | "patch" => "diff",
        "dockerfile" => "dockerfile",
        "make" | "mk" => "makefile",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_rust_and_python() {
        assert_eq!(lang_for("rs"), Some("rust"));
        assert_eq!(lang_for("py"), Some("python"));
    }

    #[test]
    fn unknown_returns_none() {
        assert_eq!(lang_for("qwerty"), None);
    }
}
