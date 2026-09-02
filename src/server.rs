//! 服务器模块（成员A负责）
//!
//! 第2天：stdin 主循环（本地模式）。
//! 第3天：TCP 网络模式 + 持久化接入——
//!   - TcpListener 监听、逐行读请求、执行、写回响应；
//!   - 启动时 recover 恢复数据，写操作先写日志再更新内存；
//!   - 处理消息分段、超长请求、非法文本、客户端断开。
//! 第4天：多客户端并发（每连接一线程 + Arc<Mutex<Server>>）。
//!
//! 依赖（已合并）：
//!   - parser::parse_command（成员C）
//!   - store::KVStore（成员B）
//!   - persistence::Persistence（成员B）——追加日志与启动恢复
//!   - protocol 常量：DEFAULT_ADDR / DEFAULT_DATA_FILE / MAX_MSG_LEN

use crate::parser;
use crate::persistence::{LogRecord, Persistence};
use crate::protocol::error;
use crate::protocol::Command;
use crate::protocol::{DEFAULT_ADDR, DEFAULT_DATA_FILE};
use crate::store::KVStore;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};

/// 服务器：封装内存存储与持久化，execute 为统一执行入口。
///
/// 第4天并发改造点：改为 `Arc<Mutex<Server>>` 由各连接线程共享，
/// execute 内的数据操作天然在锁保护下执行。
pub struct Server {
    store: KVStore,
    persistence: Persistence,
}

impl Server {
    /// 新建服务器（指定数据文件路径；数据恢复在 run 中通过 recover() 完成）
    pub fn new(data_file: &str) -> Self {
        Server {
            store: KVStore::new(),
            persistence: Persistence::new(data_file),
        }
    }

    /// 启动时恢复数据；文件异常返回 Err（调用方应报错退出，绝不静默清空）
    pub fn recover(&mut self) -> Result<(), String> {
        self.persistence.recover(&mut self.store)
    }

    /// 执行一条已解析的命令，返回要打印的响应；返回 None 表示退出服务器
    ///
    /// 持久化规则（第4阶段要求）：
    ///   - SET / DEL：先写日志文件，成功后再更新内存，最后返回成功
    ///     （保证客户端收到成功时数据已可靠落盘，重启不丢失）；
    ///   - DEL 键不存在：属于"无实际修改"，不写日志，直接返回"键不存在"；
    ///   - 写日志失败：返回明确错误，不更新内存（不返回虚假成功）。
    ///
    /// 所有错误转成 "ERROR <说明>" 文本返回，绝不 panic，保证循环继续。
    pub fn execute(&mut self, parsed: &parser::ParsedCommand) -> Option<String> {
        let cmd = parsed.command;
        let reply = match cmd {
            Command::Set => {
                let (key, value) = match (&parsed.key, &parsed.value) {
                    (Some(k), Some(v)) => (k.as_str(), v.as_str()),
                    _ => return Some(format!("ERROR {}", error::MISSING_ARG)),
                };
                // 1) 先写日志
                if let Err(e) = self.persistence.append(&LogRecord::Set {
                    key: key.to_string(),
                    value: value.to_string(),
                }) {
                    return Some(format!("ERROR 写日志失败: {}", e));
                }
                // 2) 后更新内存
                match self.store.set(key, value) {
                    Ok(()) => "OK".to_string(),
                    Err(e) => format!("ERROR {}", e),
                }
            }
            Command::Get => {
                let key = match &parsed.key {
                    Some(k) => k.as_str(),
                    None => return Some(format!("ERROR {}", error::MISSING_ARG)),
                };
                match self.store.get(key) {
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
                // 键不存在：无实际修改，不写日志
                match self.store.get(key) {
                    Ok(None) => return Some(format!("ERROR {}", error::KEY_NOT_FOUND)),
                    Err(e) => return Some(format!("ERROR {}", e)),
                    Ok(Some(_)) => {}
                }
                // 1) 先写日志
                if let Err(e) = self.persistence.append(&LogRecord::Del {
                    key: key.to_string(),
                }) {
                    return Some(format!("ERROR 写日志失败: {}", e));
                }
                // 2) 后更新内存
                match self.store.delete(key) {
                    Ok(true) => "OK".to_string(),
                    Ok(false) => format!("ERROR {}", error::KEY_NOT_FOUND),
                    Err(e) => format!("ERROR {}", e),
                }
            }
            Command::List => {
                let keys = self.store.list();
                if keys.is_empty() {
                    "KEYS".to_string()
                } else {
                    format!("KEYS {}", keys.join(" "))
                }
            }
            Command::Status => format!("STATUS count={}", self.store.len()),
            Command::Ping => "PONG".to_string(),
            Command::Exit => return None, // 收到 EXIT，退出服务器
        };
        Some(reply)
    }
}

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
    let mut server = Server::new(DEFAULT_DATA_FILE);
    if let Err(e) = server.recover() {
        eprintln!("启动失败: 数据文件异常 - {}", e);
        std::process::exit(1);
    }
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

        match server.execute(&parsed) {
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

    // 启动恢复：数据文件损坏/格式异常时明确报错退出，绝不静默清空
    let mut server = Server::new(DEFAULT_DATA_FILE);
    if let Err(e) = server.recover() {
        eprintln!("启动失败: 数据文件异常 - {}", e);
        std::process::exit(1);
    }

    println!("kvstore 服务器已启动（第3天：网络模式）");
    println!("监听地址: {}", DEFAULT_ADDR);
    println!("数据文件: {}", DEFAULT_DATA_FILE);
    println!("启动状态: 正在监听，等待客户端连接...");
    println!();

    loop {
        match listener.accept() {
            Ok((stream, addr)) => {
                println!("[连接] {} 已连接", addr);
                handle_connection(stream, &mut server);
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
/// 第4天改造点：本函数改成接收 `Arc<Mutex<Server>>`，
/// 在 run_network 中 `thread::spawn(move || handle_connection(stream, server))`。
fn handle_connection(stream: TcpStream, server: &mut Server) {
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
        match server.execute(&parsed) {
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

    /// 每个测试独立的临时数据文件，避免并发测试互相干扰
    fn make_server(tag: &str) -> Server {
        let path = std::env::temp_dir().join(format!(
            "kvstore_ut_{}_{}.log",
            std::process::id(),
            tag
        ));
        let _ = std::fs::remove_file(&path);
        Server::new(path.to_str().unwrap())
    }

    #[test]
    fn ping_returns_pong() {
        let mut server = make_server("ping");
        let reply = server.execute(&cmd(Command::Ping, None, None)).unwrap();
        assert_eq!(reply, "PONG");
    }

    #[test]
    fn status_reports_count() {
        let mut server = make_server("status");
        let reply = server.execute(&cmd(Command::Status, None, None)).unwrap();
        assert_eq!(reply, "STATUS count=0");
    }

    #[test]
    fn list_empty_store_returns_keystoken() {
        let mut server = make_server("list_empty");
        let reply = server.execute(&cmd(Command::List, None, None)).unwrap();
        assert_eq!(reply, "KEYS");
    }

    #[test]
    fn set_returns_ok_and_persists() {
        let mut server = make_server("set");
        let reply = server.execute(&cmd(Command::Set, Some("course"), Some("Rust"))).unwrap();
        assert_eq!(reply, "OK");
        // 写入后应能在内存中查到
        let reply = server.execute(&cmd(Command::Get, Some("course"), None)).unwrap();
        assert_eq!(reply, "VALUE course Rust");
    }

    #[test]
    fn get_missing_key_reports_error() {
        let mut server = make_server("get_missing");
        let reply = server.execute(&cmd(Command::Get, Some("nope"), None)).unwrap();
        assert_eq!(reply, format!("ERROR {}", error::KEY_NOT_FOUND));
    }

    #[test]
    fn del_missing_key_reports_error_without_log() {
        let mut server = make_server("del_missing");
        let reply = server.execute(&cmd(Command::Del, Some("nope"), None)).unwrap();
        assert_eq!(reply, format!("ERROR {}", error::KEY_NOT_FOUND));
    }

    #[test]
    fn missing_key_do_not_panic() {
        let mut server = make_server("missing_key");
        // GET 缺 key：应返回"缺少参数"而不是 panic（防御分支）
        let reply = server.execute(&cmd(Command::Get, None, None)).unwrap();
        assert_eq!(reply, format!("ERROR {}", error::MISSING_ARG));
    }

    #[test]
    fn exit_returns_none() {
        let mut server = make_server("exit");
        assert!(server.execute(&cmd(Command::Exit, None, None)).is_none());
    }

    #[test]
    fn recover_restores_written_data() {
        // 模拟：写入后创建新 Server（相当于重启），recover 应恢复数据
        let path = std::env::temp_dir().join(format!(
            "kvstore_ut_{}_recover.log",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        let mut server = Server::new(path.to_str().unwrap());
        server.execute(&cmd(Command::Set, Some("k1"), Some("v1"))).unwrap();
        server.execute(&cmd(Command::Set, Some("k2"), Some("v2"))).unwrap();
        drop(server); // 关闭（模拟服务器退出）

        // 重启：新 Server 实例 + recover
        let mut restarted = Server::new(path.to_str().unwrap());
        restarted.recover().unwrap();
        let reply = restarted.execute(&cmd(Command::Get, Some("k1"), None)).unwrap();
        assert_eq!(reply, "VALUE k1 v1");
        let reply = restarted.execute(&cmd(Command::Get, Some("k2"), None)).unwrap();
        assert_eq!(reply, "VALUE k2 v2");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn recover_replays_delete() {
        let path = std::env::temp_dir().join(format!(
            "kvstore_ut_{}_recover_del.log",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);

        let mut server = Server::new(path.to_str().unwrap());
        server.execute(&cmd(Command::Set, Some("k"), Some("v"))).unwrap();
        server.execute(&cmd(Command::Del, Some("k"), None)).unwrap();
        drop(server);

        let mut restarted = Server::new(path.to_str().unwrap());
        restarted.recover().unwrap();
        let reply = restarted.execute(&cmd(Command::Get, Some("k"), None)).unwrap();
        assert_eq!(reply, format!("ERROR {}", error::KEY_NOT_FOUND));
        let _ = std::fs::remove_file(&path);
    }
}
