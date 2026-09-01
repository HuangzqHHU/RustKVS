//! 服务器模块（成员A负责）
//!
//! 第2天：stdin 主循环（本地模式）。
//! 第3天：TCP 网络模式——TcpListener 监听、逐行读请求、执行、写回响应；
//!        处理消息分段（BufReader::read_line）、超长请求、非法文本、客户端断开。
//! 第4天：多客户端并发（每连接一线程 + Arc<Mutex<KVStore>>）。
//!
//! 依赖（已合并）：
//!   - parser::parse_command（成员C）
//!   - store::KVStore（成员B）
//!   - protocol 常量：DEFAULT_ADDR / MAX_MSG_LEN

use crate::parser;
use crate::protocol::error;
use crate::protocol::Command;
use crate::protocol::DEFAULT_ADDR;
use crate::store::KVStore;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};

/// 启动服务器。
/// 默认：网络模式（第3天）；传 `--local`：本地 stdin 模式（第2天，调试/演示用）。
pub fn run(args: &[String]) {
    if args.iter().any(|a| a == "--local") {
        run_local();
    } else {
        run_network();
    }
}

/// 第2天模式：本地 stdin 主循环（无网络）
fn run_local() {
    let mut store = KVStore::new();
    println!("kvstore 服务器已启动（本地模式，无网络）");
    println!("输入命令开始操作，输入 EXIT 退出，Ctrl+Z 回车可强制结束。");
    println!();

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        // 提示符（print! 不自动刷新，必须 flush）
        print!("kvstore> ");
        let _ = stdout.flush();

        let mut line = String::new();
        let n = stdin.lock().read_line(&mut line).unwrap_or(0);
        if n == 0 {
            // EOF：Ctrl+Z 回车
            println!();
            break;
        }
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }

        let parsed = match parser::parse_command(line) {
            Ok(p) => p,
            Err(e) => {
                println!("ERROR {}", e.message);
                continue;
            }
        };

        match execute(&mut store, &parsed) {
            Some(reply) => println!("{}", reply),
            None => break,
        }
    }
    println!("服务器已退出");
}

/// 第3天模式：TCP 网络服务
///
/// 第3天为单客户端串行处理：一个连接处理完（EXIT 或断开）再接下一个。
/// 第4天改为每连接一线程并发处理（见 handle_connection 注释）。
fn run_network() {
    let listener = match TcpListener::bind(DEFAULT_ADDR) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("监听 {} 失败: {}", DEFAULT_ADDR, e);
            std::process::exit(1);
        }
    };
    println!("kvstore 服务器已启动（第3天：网络模式）");
    println!("监听地址: {}", DEFAULT_ADDR);
    println!("启动状态: 正在监听，等待客户端连接...");
    println!();

    let mut store = KVStore::new();

    loop {
        match listener.accept() {
            Ok((stream, addr)) => {
                println!("[连接] {} 已连接", addr);
                handle_connection(stream, &mut store);
                println!("[连接] {} 已断开", addr);
            }
            Err(e) => {
                // 单个 accept 失败不影响服务器继续监听
                eprintln!("[错误] 接受连接失败: {}", e);
            }
        }
    }
}

/// 处理一个客户端连接：逐行读请求 → 解析 → 执行 → 写回响应
///
/// - BufReader::read_line 自动处理 TCP 消息分段（半包/粘包），无需自拼缓冲；
/// - 超长请求由 parser 的 MAX_MSG_LEN 校验并回 ERROR；
/// - 客户端断开（read_line 返回 0）或收到 EXIT 时结束本连接；
/// - 单连接内一条命令出错不影响后续命令。
///
/// 第4天改造点：本函数改成接收 `Arc<Mutex<KVStore>>`，
/// 在 run_network 中 `thread::spawn(move || handle_connection(stream, store))`。
fn handle_connection(stream: TcpStream, store: &mut KVStore) {
    // 读写各持一个句柄：BufReader 读，原 stream 写
    let read_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[错误] 克隆连接失败: {}", e);
            return;
        }
    };
    let mut reader = BufReader::new(read_stream);
    let mut writer = stream;

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break, // 客户端已断开（EOF）
            Ok(_) => {}
            Err(e) => {
                eprintln!("[错误] 读取请求失败: {}", e);
                break;
            }
        }
        let line = line.trim_end(); // 去掉 \r\n
        if line.is_empty() {
            continue; // 空行忽略
        }

        // 解析；错误回写 ERROR 后继续处理下一条
        let parsed = match parser::parse_command(line) {
            Ok(p) => p,
            Err(e) => {
                if send_line(&mut writer, &format!("ERROR {}", e.message)).is_err() {
                    break;
                }
                continue;
            }
        };

        // 执行；None 表示收到 EXIT，关闭本连接
        match execute(store, &parsed) {
            Some(reply) => {
                if send_line(&mut writer, &reply).is_err() {
                    break;
                }
            }
            None => break,
        }
    }
}

/// 向客户端写入一行响应（自动补换行符）并刷新；失败返回 Err（客户端可能已断开）
fn send_line(writer: &mut TcpStream, reply: &str) -> std::io::Result<()> {
    writer.write_all(reply.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()
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
