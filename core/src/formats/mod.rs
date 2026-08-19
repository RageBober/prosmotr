//! Парсеры форматов.
//!
//! Правило одно: каждый модуль отдаёт [`crate::Doc`] и ничего не знает
//! про интерфейс. Поэтому все они покрываются обычными юнит-тестами
//! без запуска окна.

pub mod code;
pub mod data;
pub mod excel;
pub mod markdown;
pub mod word;
pub mod table;
pub mod tree;
