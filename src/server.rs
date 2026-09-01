//! 服务器模块（成员A负责）
//!
//! 第2天：实现主循环：stdin读命令 → parser解析 → store执行 → 打印结果；
//!        保证单条命令出错后程序继续运行。
//! 第3天：接入 TcpListener 监听 DEFAULT_ADDR，每连接一线程。
//! 第4天：启动参数化（端口、数据文件路径）；并发安全共享；错误隔离。
//!
//! 依赖（已合并，全部可用）：
//!   - parser::parse_command（成员C）：一行输入 → ParsedCommand / ParseError
//!   - store::KVStore（成员B）：内存增删改查
//!
//! 接口说明：成员C第2天交付时调整了接口（parse→parse_command，
//! 字段 args→key/value），本模块已适配；接口变更见 DESIGN.md。

use crate::parser;
use crate::protocol::error;
use crate::protocol::Command;
use crate::store::KVStore;
use std::io::{self, BufRead, Write};

/// 启动服务器（第2天：本地 stdin 主循环，无网络）
pub fn run() {
    let mut store = KVStore::new();
    println!("kvstore 服务器已启动（第2天：本地模式，无网络）");
    println!("输入命令开始操作，输入 EXIT 退出，Ctrl+Z 回车可强制结束。");
    println!();

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        // 1) 打印提示符并刷新（print! 不会自动刷新，必须 flush 才会显示）
        print!("kvstore> ");
        let _ = stdout.flush();

        // 2) 读取一行用户输入
        let mut line = String::new();
        let n = stdin.lock().read_line(&mut line).unwrap_or(0);
        if n == 0 {
            // EOF：Windows 下按 Ctrl+Z 再回车触发，正常退出
            println!();
            break;
        }
        let line = line.trim_end();
        if line.is_empty() {
            continue; // 空行忽略，不当作错误
        }

        // 3) 解析（成员C的 parser）
        let parsed = match parser::parse_command(line) {
            Ok(p) => p,
            Err(e) => {
                println!("ERROR {}", e.message);
                continue; // 单条命令出错，继续下一条
            }
        };

        // 4) 执行并打印结果；None 表示收到 EXIT
        match execute(&mut store, &parsed) {
            Some(reply) => println!("{}", reply),
            None => break,
        }
    }
    println!("服务器已退出");
}

/// 执行一条已解析的命令，返回要打印的响应；返回 None 表示退出服务器
///
/// pub：供 main 的本地客户端（第2天，无网络）复用同一套执行逻辑。
/// 关键点：store 以 `&mut` 传入，execute 可以修改数据；
/// 所有错误都转成 "ERROR <说明>" 文本返回，绝不 panic，保证循环继续。
///
/// 适配成员C的接口：参数从 ParsedCommand 的 key/value 字段读取；
/// 防御处理：key/value 缺失时返回"缺少参数"，不 panic。
pub fn execute(store: &mut KVStore, parsed: &parser::ParsedCommand) -> Option<String> {
    let cmd = parsed.command;
    let reply = match cmd {
        Command::Set => {
            let (key, value) = match (&parsed.key, &parsed.value) {
                (Some(k), Some(v)) => (k.as_str(), v.as_str()),
                _ => return Some(format!("ERROR {}", error::MISSING_ARG)),
            };
            match store.set(key, value) {
                Ok(()) => "OK".to_string(),
                Err(e) => format!("ERROR {}", e),
            }
        }
        Command::Get => {
            let key = match &parsed.key {
                Some(k) => k.as_str(),
                None => return Some(format!("ERROR {}", error::MISSING_ARG)),
            };
            match store.get(key) {
                Ok(Some(v)) => format!("VALUE {} {}", key, v),
                Ok(None) => format!("ERROR {}", error::KEY_NOT_FOUND),
                Err(e) => format!("ERROR {}", e),
            }
        }
        Command::Del => {
            let key = match &parsed.key {
                Some(k) => k.as_str(),
                None => return Some(format!("ERROR {}", error::MISSING_ARG)),
            };
            match store.delete(key) {
                Ok(true) => "OK".to_string(),
                Ok(false) => format!("ERROR {}", error::KEY_NOT_FOUND),
                Err(e) => format!("ERROR {}", e),
            }
        }
        Command::List => {
            let keys = store.list();
            if keys.is_empty() {
                "KEYS".to_string()
            } else {
                format!("KEYS {}", keys.join(" "))
            }
        }
        Command::Status => format!("STATUS count={}", store.len()),
        Command::Ping => "PONG".to_string(),
        Command::Exit => return None, // 收到 EXIT，退出服务器
    };
    Some(reply)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ParsedCommand;
    use crate::protocol::Command;

    /// 构造一条已解析的命令（字段与成员C的 parser 输出一致）
    fn cmd(command: Command, key: Option<&str>, value: Option<&str>) -> ParsedCommand {
        ParsedCommand {
            command,
            key: key.map(|s| s.to_string()),
            value: value.map(|s| s.to_string()),
        }
    }

    #[test]
    fn ping_returns_pong() {
        let mut store = KVStore::new();
        let reply = execute(&mut store, &cmd(Command::Ping, None, None)).unwrap();
        assert_eq!(reply, "PONG");
    }

    #[test]
    fn status_reports_count() {
        let mut store = KVStore::new();
        let reply = execute(&mut store, &cmd(Command::Status, None, None)).unwrap();
        assert_eq!(reply, "STATUS count=0");
    }

    #[test]
    fn list_empty_store_returns_keystoken() {
        let mut store = KVStore::new();
        let reply = execute(&mut store, &cmd(Command::List, None, None)).unwrap();
        assert_eq!(reply, "KEYS");
    }

    #[test]
    fn set_returns_ok() {
        let mut store = KVStore::new();
        let reply = execute(&mut store, &cmd(Command::Set, Some("course"), Some("Rust"))).unwrap();
        assert_eq!(reply, "OK");
    }

    #[test]
    fn get_missing_key_reports_error() {
        let mut store = KVStore::new();
        let reply = execute(&mut store, &cmd(Command::Get, Some("nope"), None)).unwrap();
        assert_eq!(reply, format!("ERROR {}", error::KEY_NOT_FOUND));
    }

    #[test]
    fn del_missing_key_reports_error() {
        let mut store = KVStore::new();
        let reply = execute(&mut store, &cmd(Command::Del, Some("nope"), None)).unwrap();
        assert_eq!(reply, format!("ERROR {}", error::KEY_NOT_FOUND));
    }

    #[test]
    fn missing_key_do_not_panic() {
        let mut store = KVStore::new();
        // GET 缺 key：应返回"缺少参数"而不是 panic（防御分支）
        let reply = execute(&mut store, &cmd(Command::Get, None, None)).unwrap();
        assert_eq!(reply, format!("ERROR {}", error::MISSING_ARG));
    }

    #[test]
    fn exit_returns_none() {
        let mut store = KVStore::new();
        assert!(execute(&mut store, &cmd(Command::Exit, None, None)).is_none());
    }
}
