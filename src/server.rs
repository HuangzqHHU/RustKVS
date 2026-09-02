//! 服务器模块（成员A负责 + 成员D第4天并发改造）
//!
//! 第2天：stdin 主循环（本地模式）。
//! 第3天：TCP 网络模式——TcpListener 监听、逐行读请求、执行、写回响应。
//! 第4天：多客户端并发（每连接一线程 + Arc<Mutex<KVStore>>）。
//!
//! 并发改造说明（成员D）：
//!   - store 从 `&mut KVStore` 改为 `Arc<Mutex<KVStore>>`
//!   - run_network 中每 accept 一个连接就 thread::spawn 一个线程
//!   - handle_connection 接收 Arc<Mutex<KVStore>>，每次操作前 lock()
//!   - 锁粒度：只在 execute 调用瞬间持有锁，I/O 读写不占锁
//!   - 错误隔离：单个连接 panic 不影响服务器（catch_unwind 兜底）

use crate::parser;
use crate::protocol::error;
use crate::protocol::Command;
use crate::protocol::DEFAULT_ADDR;
use crate::store::KVStore;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

/// 启动服务器。
/// 默认：网络模式（第3天）；传 `--local`：本地 stdin 模式（第2天，调试/演示用）。
pub fn run(args: &[String]) {
    if args.iter().any(|a| a == "--local") {
        run_local();
    } else {
        run_network();
    }
}

// ============================================================
// 本地模式（第2天）
// ============================================================

fn run_local() {
    let mut store = KVStore::new();
    println!("kvstore 服务器已启动（本地模式，无网络）");
    println!("输入命令开始操作，输入 EXIT 退出，Ctrl+Z 回车可强制结束。");
    println!();

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("kvstore> ");
        let _ = stdout.flush();

        let mut line = String::new();
        let n = stdin.lock().read_line(&mut line).unwrap_or(0);
        if n == 0 {
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

// ============================================================
// 网络模式（第3天 + 第4天并发改造）
// ============================================================

/// 第4天：并发网络服务
///
/// - 每连接一线程（thread::spawn）
/// - store 用 Arc<Mutex<_>> 共享
/// - 单个连接 panic 不影响主循环
fn run_network() {
    let listener = match TcpListener::bind(DEFAULT_ADDR) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("监听 {} 失败: {}", DEFAULT_ADDR, e);
            std::process::exit(1);
        }
    };
    println!("kvstore 服务器已启动（第4天：并发网络模式）");
    println!("监听地址: {}", DEFAULT_ADDR);
    println!("启动状态: 正在监听，等待客户端连接...");
    println!();

    // 用 Arc<Mutex> 包装 store，多线程安全共享
    let store = Arc::new(Mutex::new(KVStore::new()));

        for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let addr = match stream.peer_addr() {
                    Ok(a) => a.to_string(),
                    Err(_) => "未知地址".to_string(),
                };
                println!("[连接] {} 已连接", addr);
                let store_clone = Arc::clone(&store);
                thread::spawn(move || {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        handle_connection(stream, store_clone);
                    }));
                    if let Err(_) = result {
                        eprintln!("[警告] 客户端线程 panic 已捕获");
                    }
                    println!("[连接] {} 已断开", addr);
                });
            }
            Err(e) => {
                eprintln!("[错误] 接受连接失败: {}", e);
            }
        }
    }
}

/// 处理一个客户端连接（第4天：接收 Arc<Mutex<KVStore>>）
///
/// - BufReader::read_line 自动处理 TCP 消息分段
/// - 超长请求由 parser 校验并回 ERROR
/// - 客户端断开或收到 EXIT 时结束
/// - 单连接内一条命令出错不影响后续命令
/// - 锁只在 execute 调用时持有，I/O 不占锁
fn handle_connection(stream: TcpStream, store: Arc<Mutex<KVStore>>) {
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
            Ok(0) => break,
            Ok(_) => {}
            Err(e) => {
                eprintln!("[错误] 读取请求失败: {}", e);
                break;
            }
        }
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }

        // 解析：不持有锁
        let parsed = match parser::parse_command(line) {
            Ok(p) => p,
            Err(e) => {
                if send_line(&mut writer, &format!("ERROR {}", e.message)).is_err() {
                    break;
                }
                continue;
            }
        };

        // 执行：只在这一步持有锁（锁粒度最小化）
        let reply = {
            let mut store_guard = store.lock().unwrap();
            execute(&mut store_guard, &parsed)
        };

        match reply {
            Some(reply_text) => {
                if send_line(&mut writer, &reply_text).is_err() {
                    break;
                }
            }
            None => break, // EXIT
        }
    }
}

/// 向客户端写入一行响应并刷新
fn send_line(writer: &mut TcpStream, reply: &str) -> std::io::Result<()> {
    writer.write_all(reply.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()
}

// ============================================================
// 命令执行（共享逻辑）
// ============================================================

/// 执行一条已解析的命令，返回响应文本；返回 None 表示退出连接
///
/// pub：供集成测试和本地模式复用同一套执行逻辑。
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
        Command::Exit => return None,
    };
    Some(reply)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ParsedCommand;
    use crate::protocol::Command;

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
        let reply = execute(&mut store, &cmd(Command::Get, None, None)).unwrap();
        assert_eq!(reply, format!("ERROR {}", error::MISSING_ARG));
    }

    #[test]
    fn exit_returns_none() {
        let mut store = KVStore::new();
        assert!(execute(&mut store, &cmd(Command::Exit, None, None)).is_none());
    }
}