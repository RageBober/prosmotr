//! json → форма `Tree`.
//!
//! Разбор оставляем serde_json, а раскрытие узлов — интерфейсу:
//! он и так умеет показывать вложенность, дублировать её в Rust незачем.

use crate::Doc;

pub fn from_json(src: &str) -> Doc {
    match serde_json::from_str::<serde_json::Value>(src) {
        Ok(json) => Doc::Tree { json },
        Err(e) => Doc::Unsupported {
            ext: "json".into(),
            message: format!("Битый JSON: строка {}, столбец {} — {}", e.line(), e.column(), e),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_object() {
        match from_json(r#"{"имя":"Алфер","часы":[475,800]}"#) {
            Doc::Tree { json } => {
                assert_eq!(json["имя"], "Алфер");
                assert_eq!(json["часы"][1], 800);
            }
            other => panic!("ожидали Tree, получили {other:?}"),
        }
    }

    #[test]
    fn broken_json_reports_position() {
        match from_json("{\n  \"a\": ,\n}") {
            Doc::Unsupported { message, .. } => assert!(message.contains("строка 2")),
            other => panic!("ожидали Unsupported, получили {other:?}"),
        }
    }
}
