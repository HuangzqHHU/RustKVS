//! 服务器模块（成员A负责）
//!
//! 第2天：实现主循环：stdin读命令 → parser解析 → store执行 → 打印结果；
//!        保证单条命令出错后程序继续运行。
//! 第3天：接入 TcpListener 监听 DEFAULT_ADDR，每连接一线程。
//! 第4天：启动参数化（端口、数据文件路径）；并发安全共享；错误隔离。
//!
//! 依赖（接口已定稿，成员B/C实现后自动生效）：
//!   - parser::parse（成员C）：一行输入 → ParsedCommand / ParseError
//!   - store::KVStore（成员B）：内存增删改查

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

        // 3) 解析（成员C的 parser；集成前会提示"解析器待实现"）
        let parsed = match parser::parse(line) {
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
/// 关键点：store 以 `&mut` 传入，execute 可以修改数据；
/// 所有错误都转成 "ERROR <说明>" 文本返回，绝不 panic，保证循环继续。
fn execute(store: &mut KVStore, parsed: &parser::ParsedCommand) -> Option<String> {
    let cmd = parsed.cmd;
    let args = &parsed.args;

    // 防御：参数个数不足时不 panic，返回明确错误（正常应由 parser 拦截）
    if args.len() < cmd.required_args() {
        return Some(format!("ERROR {}", error::MISSING_ARG));
    }

    let reply = match cmd {
        Command::Set => {
            let key = &args[0];
            let value = &args[1];
            match store.set(key, value) {
                Ok(()) => "OK".to_string(),
                Err(e) => format!("ERROR {}", e),
            }
        }
        Command::Get => {
            let key = &args[0];
            match store.get(key) {
                Ok(Some(v)) => format!("VALUE {} {}", key, v),
                Ok(None) => format!("ERROR {}", error::KEY_NOT_FOUND),
                Err(e) => format!("ERROR {}", e),
            }
        }
        Command::Del => {
            let key = &args[0];
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

    /// 构造一条已解析的命令（第2天起成员C的 parser 会返回同款结构）
    fn cmd(command: Command, args: &[&str]) -> ParsedCommand {
        ParsedCommand {
            cmd: command,
            args: args.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn ping_returns_pong() {
        let mut store = KVStore::new();
        let reply = execute(&mut store, &cmd(Command::Ping, &[])).unwrap();
        assert_eq!(reply, "PONG");
    }

    #[test]
    fn status_reports_count() {
        let mut store = KVStore::new();
        let reply = execute(&mut store, &cmd(Command::Status, &[])).unwrap();
        assert_eq!(reply, "STATUS count=0");
    }

    #[test]
    fn list_empty_store_returns_keystoken() {
        let mut store = KVStore::new();
        let reply = execute(&mut store, &cmd(Command::List, &[])).unwrap();
        assert_eq!(reply, "KEYS");
    }

    #[test]
    fn set_returns_ok() {
        let mut store = KVStore::new();
        let reply = execute(&mut store, &cmd(Command::Set, &["course", "Rust"])).unwrap();
        assert_eq!(reply, "OK");
    }

    #[test]
    fn get_missing_key_reports_error() {
        let mut store = KVStore::new();
        let reply = execute(&mut store, &cmd(Command::Get, &["nope"])).unwrap();
        assert_eq!(reply, format!("ERROR {}", error::KEY_NOT_FOUND));
    }

    #[test]
    fn del_missing_key_reports_error() {
        let mut store = KVStore::new();
        let reply = execute(&mut store, &cmd(Command::Del, &["nope"])).unwrap();
        assert_eq!(reply, format!("ERROR {}", error::KEY_NOT_FOUND));
    }

    #[test]
    fn missing_args_do_not_panic() {
        let mut store = KVStore::new();
        // GET 缺参数：应返回"缺少参数"而不是 panic（防御分支）
        let reply = execute(&mut store, &cmd(Command::Get, &[])).unwrap();
        assert_eq!(reply, format!("ERROR {}", error::MISSING_ARG));
    }

    #[test]
    fn exit_returns_none() {
        let mut store = KVStore::new();
        assert!(execute(&mut store, &cmd(Command::Exit, &[])).is_none());
    }
}
