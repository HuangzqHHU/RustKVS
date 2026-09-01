//! 命令行客户端模块（成员C负责）
//!
//! 第1天：定义入口函数签名（骨架，可编译）。
use std::io::{self, Write};

pub fn run_repl_skeleton() {
    loop {
        print!("kv> ");
        io::stdout().flush().unwrap();

        let mut input = String::new();

        match io::stdin().read_line(&mut input) {
            Ok(0) => {
                println!("输入结束，客户端退出。");
                break;
            }
            Ok(_) => {
                println!("你输入的是：{}", input.trim());
            }
            Err(error) => {
                eprintln!("读取输入失败：{}", error);
                break;
            }
        }
    }
}