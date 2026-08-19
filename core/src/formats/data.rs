//! yaml / toml / xml → форма `Tree`.
//!
//! yaml и toml разбираются прямо в `serde_json::Value` — их парсеры это умеют,
//! промежуточные типы не нужны. xml приходится складывать руками: у него есть
//! атрибуты и повторяющиеся теги, которых в json нет.

use crate::Doc;
use quick_xml::events::Event;
use quick_xml::Reader;
use serde_json::{Map, Value};

pub fn from_yaml(src: &str) -> Doc {
    match serde_yaml::from_str::<Value>(src) {
        Ok(json) => Doc::Tree { json },
        Err(e) => Doc::Unsupported {
            ext: "yaml".into(),
            message: format!("Не разобрался в yaml: {e}"),
        },
    }
}

pub fn from_toml(src: &str) -> Doc {
    match toml::from_str::<Value>(src) {
        Ok(json) => Doc::Tree { json },
        Err(e) => Doc::Unsupported {
            ext: "toml".into(),
            message: format!("Не разобрался в toml: {e}"),
        },
    }
}

struct El {
    name: String,
    attrs: Vec<(String, String)>,
    kids: Vec<El>,
    text: String,
}

impl El {
    fn new(name: String) -> Self {
        Self { name, attrs: vec![], kids: vec![], text: String::new() }
    }

    fn into_value(self) -> Value {
        // лист без атрибутов — просто строка, иначе дерево утонет в скобках
        if self.kids.is_empty() && self.attrs.is_empty() {
            return Value::String(self.text);
        }
        let mut map = Map::new();
        for (k, v) in self.attrs {
            map.insert(format!("@{k}"), Value::String(v));
        }
        if !self.text.is_empty() {
            map.insert("#текст".into(), Value::String(self.text));
        }
        for kid in self.kids {
            let name = kid.name.clone();
            let val = kid.into_value();
            match map.get_mut(&name) {
                // одноимённые теги собираем в список
                Some(Value::Array(arr)) => arr.push(val),
                Some(existing) => {
                    let prev = existing.take();
                    map.insert(name, Value::Array(vec![prev, val]));
                }
                None => {
                    map.insert(name, val);
                }
            }
        }
        Value::Object(map)
    }
}

fn build(e: &quick_xml::events::BytesStart) -> El {
    let mut el = El::new(String::from_utf8_lossy(e.name().as_ref()).into_owned());
    for a in e.attributes().flatten() {
        el.attrs.push((
            String::from_utf8_lossy(a.key.as_ref()).into_owned(),
            String::from_utf8_lossy(&a.value).into_owned(),
        ));
    }
    el
}

pub fn from_xml(src: &str) -> Doc {
    let mut r = Reader::from_str(src);
    r.config_mut().trim_text(true);

    let mut stack: Vec<El> = Vec::new();
    let mut root: Option<El> = None;

    loop {
        match r.read_event() {
            Ok(Event::Start(e)) => stack.push(build(&e)),
            // самозакрывающийся тег закрывается сразу же, иначе следующие
            // соседи ошибочно станут его детьми
            Ok(Event::Empty(e)) => {
                let el = build(&e);
                match stack.last_mut() {
                    Some(parent) => parent.kids.push(el),
                    None => root = Some(el),
                }
            }
            Ok(Event::Text(t)) => {
                if let Some(top) = stack.last_mut() {
                    let s = t.unescape().unwrap_or_default();
                    if !s.trim().is_empty() {
                        top.text.push_str(s.trim());
                    }
                }
            }
            Ok(Event::End(_)) => {
                if let Some(done) = stack.pop() {
                    match stack.last_mut() {
                        Some(parent) => parent.kids.push(done),
                        None => root = Some(done),
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Doc::Unsupported {
                    ext: "xml".into(),
                    message: format!("Битый xml: {e}"),
                }
            }
            _ => {}
        }
    }

    // незакрытые теги (или самозакрывающиеся) сворачиваем снизу вверх
    while let Some(done) = stack.pop() {
        match stack.last_mut() {
            Some(parent) => parent.kids.push(done),
            None => root = Some(done),
        }
    }

    match root {
        Some(el) => {
            let name = el.name.clone();
            let mut map = Map::new();
            map.insert(name, el.into_value());
            Doc::Tree { json: Value::Object(map) }
        }
        None => Doc::Unsupported {
            ext: "xml".into(),
            message: "В файле нет ни одного тега".into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(d: Doc) -> Value {
        match d {
            Doc::Tree { json } => json,
            other => panic!("ожидали Tree, получили {other:?}"),
        }
    }

    #[test]
    fn yaml_becomes_tree() {
        let v = tree(from_yaml("имя: Алфер\nчасы:\n  - 475\n  - 800\n"));
        assert_eq!(v["имя"], "Алфер");
        assert_eq!(v["часы"][1], 800);
    }

    #[test]
    fn toml_becomes_tree() {
        let v = tree(from_toml("[package]\nname = \"prosmotr\"\nyear = 2026\n"));
        assert_eq!(v["package"]["name"], "prosmotr");
        assert_eq!(v["package"]["year"], 2026);
    }

    #[test]
    fn toml_cyrillic_keys_need_quotes() {
        // так требует сам формат: голые ключи только латиницей.
        // Без кавычек парсер ругается — и мы показываем его слова пользователю.
        assert!(matches!(from_toml("[план]\n"), Doc::Unsupported { .. }));
        let v = tree(from_toml("[\"план\"]\n\"вуз\" = \"METU\"\n"));
        assert_eq!(v["план"]["вуз"], "METU");
    }

    #[test]
    fn xml_keeps_attributes_and_text() {
        let v = tree(from_xml(r#"<план год="2026"><вуз>METU</вуз></план>"#));
        assert_eq!(v["план"]["@год"], "2026");
        assert_eq!(v["план"]["вуз"], "METU");
    }

    #[test]
    fn xml_repeated_tags_become_array() {
        let v = tree(from_xml("<root><a>1</a><a>2</a><a>3</a></root>"));
        assert!(v["root"]["a"].is_array());
        assert_eq!(v["root"]["a"][2], "3");
    }

    #[test]
    fn self_closing_tags_are_siblings_not_children() {
        // <a/><b/> — это соседи; когда-то b оказывался внутри a
        let v = tree(from_xml(r#"<root><a x="1"/><b y="2"/></root>"#));
        assert_eq!(v["root"]["a"]["@x"], "1");
        assert_eq!(v["root"]["b"]["@y"], "2");
    }

    #[test]
    fn broken_yaml_is_reported() {
        assert!(matches!(from_yaml("a:\n  - b\n c: ["), Doc::Unsupported { .. }));
    }
}
