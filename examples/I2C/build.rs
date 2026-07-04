//! # 构建脚本 (build.rs)
//!
//! 这个文件在编译时运行，用于配置链接器参数。
//! 它的主要作用：
//! 1. 添加链接器脚本 linkall.x
//! 2. 提供友好的错误提示（当缺少必要的依赖时）
//!
//! ## 工作原理
//! - 当直接运行时（无参数）：配置链接器脚本
//! - 当被链接器调用时（有参数）：检查并提示缺失的符号

fn main() {
    // 调用友好的错误处理函数
    linker_be_nice();

    // 添加 ESP32 链接器脚本
    // linkall.x 包含了 ESP32 启动和内存布局的配置
    // 注意：必须放在最后，否则可能与 flip-link 冲突
    println!("cargo:rustc-link-arg=-Tlinkall.x");
}

/// 链接器友好的错误处理
///
/// 当链接器遇到未定义的符号时，会调用这个脚本。
/// 我们检查常见的错误，并提供有用的提示信息。
///
/// # 工作流程
/// 1. 直接运行：注册为错误处理脚本
/// 2. 被链接器调用：检查缺失的符号并提示
fn linker_be_nice() {
    let args: Vec<String> = std::env::args().collect();

    // 如果有参数，说明是被链接器调用来处理错误
    if args.len() > 1 {
        let kind = &args[1];  // 错误类型
        let what = &args[2];  // 缺失的符号

        match kind.as_str() {
            "undefined-symbol" => match what.as_str() {
                // 缺少 defmt（嵌入式调试框架）
                what if what.starts_with("_defmt_") => {
                    eprintln!();
                    eprintln!(
                        "💡 `defmt` not found - make sure `defmt.x` is added as a linker script and you have included `use defmt_rtt as _;`"
                    );
                    eprintln!();
                }

                // 缺少栈起始地址（通常是链接器脚本问题）
                "_stack_start" => {
                    eprintln!();
                    eprintln!("💡 Is the linker script `linkall.x` missing?");
                    eprintln!();
                }

                // 缺少 ESP RTOS 调度器
                what if what.starts_with("esp_rtos_") => {
                    eprintln!();
                    eprintln!(
                        "💡 `esp-radio` has no scheduler enabled. Make sure you have initialized `esp-rtos` or provided an external scheduler."
                    );
                    eprintln!();
                }

                // 缺少嵌入式测试支持
                "embedded_test_linker_file_not_added_to_rustflags" => {
                    eprintln!();
                    eprintln!(
                        "💡 `embedded-test` not found - make sure `embedded-test.x` is added as a linker script for tests"
                    );
                    eprintln!();
                }

                // 缺少内存分配器
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

                // 其他未定义符号
                _ => (),
            },

            // 其他类型的错误，直接退出
            _ => {
                std::process::exit(1);
            }
        }

        // 错误已处理，正常退出
        std::process::exit(0);
    }

    // 如果没有参数，说明是正常编译流程
    // 注册自己为链接器错误处理脚本
    // 这样当链接器遇到错误时，会调用我们来提供友好的提示
    println!(
        "cargo:rustc-link-arg=-Wl,--error-handling-script={}",
        std::env::current_exe().unwrap().display()
    );
}
