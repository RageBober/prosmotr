//! Чтение текста с диска: кодировки и защита от гигантских файлов.

use std::io::Read;
use std::path::Path;

/// Прочитать файл как текст, не больше `max_bytes`.
///
/// Возвращает `(текст, обрезан_ли)`.
///
/// Кодировка: сначала UTF-8. Если файл в неё не укладывается — windows-1251.
/// Это не роскошь: выгрузки из 1С и старые CSV из госорганов приходят
/// именно в cp1251, и без этого пользователь видит кракозябры.
pub fn read_text(path: &Path, max_bytes: usize) -> std::io::Result<(String, bool)> {
    let mut f = std::fs::File::open(path)?;
    let size = f.metadata().map(|m| m.len() as usize).unwrap_or(0);
    let cap = size.min(max_bytes.saturating_add(1));

    let mut buf = Vec::with_capacity(cap);
    let mut limited = f.by_ref().take(max_bytes.saturating_add(1) as u64);
    limited.read_to_end(&mut buf)?;

    let truncated = buf.len() > max_bytes;
    if truncated {
        buf.truncate(max_bytes);
    }

    let text = match std::str::from_utf8(&buf) {
        Ok(s) => s.to_string(),
        Err(e) => {
            // `error_len() == None` означает «последовательность оборвалась в конце» —
            // то есть мы сами разрезали символ пополам при обрезке.
            // Это не повод считать файл однобайтовым: отбрасываем хвост и всё.
            //
            // Если же ошибка в середине (`Some(_)`) — файл действительно не UTF-8,
            // и вот тогда пробуем windows-1251: так приходят выгрузки из 1С
            // и старые CSV, иначе пользователь видит кракозябры.
            if e.error_len().is_none() {
                buf.truncate(e.valid_up_to());
                String::from_utf8(buf).unwrap_or_default()
            } else {
                let (cow, _, _) = encoding_rs::WINDOWS_1251.decode(&buf);
                cow.into_owned()
            }
        }
    };
    Ok((text, truncated))
}

/// Грубая проверка «это вообще текст?» — по нулевым байтам в начале.
pub fn looks_binary(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(8192)];
    head.contains(&0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmp(name: &str, body: &[u8]) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(name);
        std::fs::File::create(&p).unwrap().write_all(body).unwrap();
        p
    }

    #[test]
    fn reads_utf8() {
        let p = tmp("u8.txt", "привет".as_bytes());
        let (s, tr) = read_text(&p, 1024).unwrap();
        assert_eq!(s, "привет");
        assert!(!tr);
    }

    #[test]
    fn falls_back_to_cp1251() {
        // «Дата» в windows-1251
        let bytes = [0xC4u8, 0xE0, 0xF2, 0xE0];
        let p = tmp("cp.txt", &bytes);
        let (s, _) = read_text(&p, 1024).unwrap();
        assert_eq!(s, "Дата");
    }

    #[test]
    fn truncates_without_breaking_utf8() {
        let p = tmp("big.txt", "ααααα".as_bytes()); // по 2 байта на символ
        let (s, tr) = read_text(&p, 5).unwrap();
        assert!(tr);
        assert_eq!(s, "αα"); // 5-й байт отброшен, символ не разрезан
    }

    #[test]
    fn truncated_utf8_does_not_become_cp1251() {
        // проверка на регресс: обрезка посреди символа однажды уже
        // превращала нормальный текст в «О±О±О»
        let p = tmp("cut.txt", "привет мир".as_bytes());
        let (s, tr) = read_text(&p, 7).unwrap();
        assert!(tr);
        assert_eq!(s, "при");
    }

    #[test]
    fn detects_binary() {
        assert!(looks_binary(&[1, 2, 0, 3]));
        assert!(!looks_binary("обычный текст".as_bytes()));
    }
}
