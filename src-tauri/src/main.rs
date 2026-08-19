// Окно без консоли в релизной сборке
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Тонкая прослойка: диалог выбора папки и три команды для интерфейса.
//! Вся работа с форматами — в крейте `prosmotr-core`, который собирается
//! и тестируется без GUI.

use std::path::PathBuf;

use prosmotr_core::{open_document, scan_folder, Doc, FileEntry, Limits};
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

/// Нативный диалог выбора папки.
///
/// Команда асинхронная сознательно: так она исполняется в рантайме Tauri,
/// а не в потоке окна, и диалог не подвешивает отрисовку.
#[tauri::command]
async fn pick_folder(app: AppHandle) -> Option<String> {
    let (tx, mut rx) = tauri::async_runtime::channel(1);
    app.dialog().file().pick_folder(move |folder| {
        let _ = tx.try_send(folder);
    });
    rx.recv()
        .await
        .flatten()
        .and_then(|f| f.into_path().ok())
        .map(|p: PathBuf| p.to_string_lossy().into_owned())
}

#[derive(serde::Serialize)]
struct Startup {
    folder: String,
    /// Если запустили с файлом, а не с папкой — сразу открываем этот файл.
    file: Option<String>,
}

/// Аргумент командной строки: `prosmotr ~/Документы` или `prosmotr план.md`.
///
/// Тем же путём приходит «Открыть с помощью» из файлового менеджера,
/// поэтому файл превращаем в его папку, а сам файл открываем во вкладке.
#[tauri::command]
fn initial_folder() -> Option<Startup> {
    let arg = std::env::args().nth(1)?;
    if arg.starts_with('-') {
        return None;
    }
    let p = PathBuf::from(&arg).canonicalize().ok()?;
    if p.is_dir() {
        return Some(Startup { folder: p.to_string_lossy().into_owned(), file: None });
    }
    let dir = p.parent()?.to_path_buf();
    Some(Startup {
        folder: dir.to_string_lossy().into_owned(),
        file: Some(p.to_string_lossy().into_owned()),
    })
}

/// Список файлов в папке — то, что видно слева.
#[tauri::command]
fn list_files(root: String) -> Result<Vec<FileEntry>, String> {
    let path = PathBuf::from(&root);
    if !path.is_dir() {
        return Err(format!("Это не папка: {root}"));
    }
    Ok(scan_folder(&path))
}

/// Открыть файл — вернётся одна из пяти форм документа.
#[tauri::command]
fn read_doc(path: String) -> Result<Doc, String> {
    open_document(&PathBuf::from(&path), Limits::default())
        .map_err(|e| format!("Не удалось прочитать {path}: {e}"))
}

/// Сырые байты файла — для картинок и pdf.
///
/// Возвращаем `Response`, а не `Vec<u8>`: иначе Tauri отправит байты
/// как json-массив чисел, и десятимегабайтный pdf раздуется вчетверо.
#[tauri::command]
fn read_bytes(path: String) -> Result<tauri::ipc::Response, String> {
    const MAX: u64 = 256 * 1024 * 1024;
    let p = PathBuf::from(&path);
    let meta = std::fs::metadata(&p).map_err(|e| format!("Нет файла {path}: {e}"))?;
    if meta.len() > MAX {
        return Err(format!(
            "Файл слишком большой: {:.0} МБ",
            meta.len() as f64 / 1_048_576.0
        ));
    }
    let bytes = std::fs::read(&p).map_err(|e| format!("Не удалось прочитать {path}: {e}"))?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// Исходный текст файла — для режима «показать как есть».
#[tauri::command]
fn read_source(path: String) -> Result<String, String> {
    prosmotr_core::text::read_text(&PathBuf::from(&path), 2 * 1024 * 1024)
        .map(|(s, truncated)| {
            if truncated {
                format!("{s}\n\n… файл обрезан на 2 МБ")
            } else {
                s
            }
        })
        .map_err(|e| format!("Не удалось прочитать {path}: {e}"))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            pick_folder,
            initial_folder,
            list_files,
            read_doc,
            read_bytes,
            read_source
        ])
        .run(tauri::generate_context!())
        .expect("не удалось запустить окно");
}
