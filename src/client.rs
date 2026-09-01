//! 命令行客户端模块（成员C负责）
//!
//! 第1天：定义入口函数签名（骨架，可编译）。
use crate::parser::{parse_command, ParsedCommand};
use crate::protocol::Command;
use std::io::{self, Write};

pub fn run_local_repl<F>(mut execute: F)
where
 F: FnMut(ParsedCommand) -> String,
{
    loop {
        print!("kv> ");
        io::stdout().flush().unwrap();

        let mut input = String::new();

        match io::stdin().read_line(&mut input) {
            Ok(0) => {
                println!("输入结束，客户端退出。");
                break;
            }

Ok(_) => match parse_command(&input) {
    Ok(command) => {
        let is_exit = matches!(command.command, Command::Exit);

        let response = execute(command);
        println!("{}", response);

        if is_exit {
            break;
        }
    }

    Err(error) => {
        println!("{}", error);
    }
},

            Err(error) => {
                eprintln!("读取输入失败：{}", error);
                break;
            }
        }
    }
}