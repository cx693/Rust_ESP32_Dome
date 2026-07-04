//! 库 crate 根文件
//!
//! 本项目是二进制 crate（bin），但 Cargo 要求同时存在 lib.rs。
//! 此文件仅声明 no_std，无实际功能代码。
//! 主要逻辑在 `src/bin/main.rs` 中。

#![no_std]
