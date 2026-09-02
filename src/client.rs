//! 命令行客户端模块（成员C负责）
//!
//! 第1天：定义入口函数签名（骨架，可编译）。
use crate::parser::{ParsedCommand, parse_command};
use crate::protocol::Command;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

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
pub fn run_tcp_repl(address: &str) {
    let socket_address = match address.to_socket_addrs() {
        Ok(mut addresses) => match addresses.next() {
            Some(address) => address,
            None => {
                eprintln!("地址无效：{}", address);
                return;
            }
        },
        Err(error) => {
            eprintln!("地址解析失败：{}，原因：{}", address, error);
            return;
        }
    };

    let mut writer = match TcpStream::connect_timeout(&socket_address, Duration::from_secs(5)) {
        Ok(stream) => stream,
        Err(error) => {
            eprintln!("连接服务器失败：{}，原因：{}", address, error);
            return;
        }
    };

    if let Err(error) = writer.set_read_timeout(Some(Duration::from_secs(10))) {
        eprintln!("设置读取超时失败：{}", error);
        return;
    }

    let reader_stream = match writer.try_clone() {
        Ok(stream) => stream,
        Err(error) => {
            eprintln!("创建读取连接失败：{}", error);
            return;
        }
    };

    let mut reader = BufReader::new(reader_stream);

    println!("已连接到服务器：{}", address);
    println!("可用命令：SET、GET、DEL、LIST、STATUS、PING、EXIT");

    loop {
        print!("kv> ");

        if let Err(error) = io::stdout().flush() {
            eprintln!("刷新输出失败：{}", error);
            break;
        }

        let mut input = String::new();

        match io::stdin().read_line(&mut input) {
            Ok(0) => {
                println!("输入结束，客户端退出。");
                break;
            }

            Ok(_) => {}

            Err(error) => {
                eprintln!("读取输入失败：{}", error);
                break;
            }
        }

        let command = match parse_command(&input) {
            Ok(command) => command,

            Err(error) => {
                println!("{}", error);
                continue;
            }
        };

        let request = input.trim_end_matches('\n').trim_end_matches('\r');

        if let Err(error) = writer.write_all(request.as_bytes()) {
            eprintln!("发送请求失败，服务器可能已断开：{}", error);
            break;
        }

        if let Err(error) = writer.write_all(b"\n") {
            eprintln!("发送换行符失败：{}", error);
            break;
        }

        if let Err(error) = writer.flush() {
            eprintln!("发送请求失败：{}", error);
            break;
        }

        let mut response = String::new();

        match reader.read_line(&mut response) {
            Ok(0) => {
                eprintln!("服务器已主动断开连接。");
                break;
            }

            Ok(_) => {
                print!("服务器：{}", response);
            }

            Err(error) => {
                eprintln!("读取服务器响应失败：{}", error);
                break;
            }
        }

        if matches!(command.command, Command::Exit) {
            break;
        }
    }

    println!("客户端已退出。");
}
