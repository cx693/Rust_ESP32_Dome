//! 构建脚本（build script）
//!
//! Cargo 在编译前自动执行此脚本。它做两件事：
//! 1. 指定 ESP32 的链接脚本 `linkall.x`（定义内存布局、入口点等）
//! 2. 注册为链接器的错误处理脚本 —— 当出现常见未定义符号时，
//!    输出友好的中文提示，而不是晦涩的链接器错误。
//!
//! 📌 一般不需要修改此文件，了解即可。

fn main() {
    linker_be_nice();
    // linkall.x 是 esp-hal 提供的链接脚本，必须放在最后加载
    // 它定义了 ESP32-S3 的内存布局和中断向量表
    println!("cargo:rustc-link-arg=-Tlinkall.x");
}

/// 注册自身为链接器的错误诊断脚本。
///
/// 当链接器遇到未定义符号时，会调用本脚本并传入错误信息。
/// 我们识别常见错误并输出友好的修复建议。
fn linker_be_nice() {
    let args: Vec<String> = std::env::args().collect();

    // 如果有参数，说明是被链接器作为错误处理脚本调用
    if args.len() > 1 {
        let kind = &args[1]; // 错误类型，如 "undefined-symbol"
        let what = &args[2]; // 具体的未定义符号名

        match kind.as_str() {
            "undefined-symbol" => match what.as_str() {
                // 缺少 defmt 日志框架
                what if what.starts_with("_defmt_") => {
                    eprintln!();
                    eprintln!(
                        "💡 `defmt` not found - make sure `defmt.x` is added as a linker script and you have included `use defmt_rtt as _;`"
                    );
                    eprintln!();
                }
                // 缺少 linkall.x 链接脚本
                "_stack_start" => {
                    eprintln!();
                    eprintln!("💡 Is the linker script `linkall.x` missing?");
                    eprintln!();
                }
                // esp-radio 没有找到调度器
                what if what.starts_with("esp_rtos_") => {
                    eprintln!();
                    eprintln!(
                        "💡 `esp-radio` has no scheduler enabled. Make sure you have initialized `esp-rtos` or provided an external scheduler."
                    );
                    eprintln!();
                }
                // 缺少嵌入式测试框架
                "embedded_test_linker_file_not_added_to_rustflags" => {
                    eprintln!();
                    eprintln!(
                        "💡 `embedded-test` not found - make sure `embedded-test.x` is added as a linker script for tests"
                    );
                    eprintln!();
                }
                // 缺少内存分配器（esp-alloc）
                "free"
                | "malloc"
                | "calloc"
                | "get_free_internal_heap_size"
                | "malloc_internal"
                | "realloc_internal"
                | "calloc_internal"
                | "free_internal" => {
                    eprintln!();
                    eprintln!(
                        "💡 Did you forget the `esp-alloc` dependency or didn't enable the `compat` feature on it?"
                    );
                    eprintln!();
                }
                _ => (),
            },
            _ => {
                // 其他类型的链接错误，正常报错退出
                std::process::exit(1);
            }
        }

        // 错误已处理，正常退出（不让链接器继续报错）
        std::process::exit(0);
    }

    // 正常构建时：将自身注册为链接器的错误处理脚本
    // 这样链接器遇到未定义符号时就会调用我们上面的诊断逻辑
    println!(
        "cargo:rustc-link-arg=-Wl,--error-handling-script={}",
        std::env::current_exe().unwrap().display()
    );
}
