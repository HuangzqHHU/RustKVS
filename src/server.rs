//! 服务器模块（成员A负责）
//!
//! 第2天：stdin 主循环（本地模式）。
//! 第3天：TCP 网络模式 + 持久化接入——启动 recover、写操作先写日志再更新内存。
//! 第4天：多客户端并发——
//!   - 每连接一线程（thread::spawn），多个客户端同时连接并行处理；
//!   - Arc<Mutex<Server>> 共享存储状态，锁只在 execute（数据操作）期间持有；
//!   - 参数化：--port <端口>、--data <数据文件路径>。
//!
//! 依赖（已合并）：
//!   - parser::parse_command（成员C）
//!   - store::KVStore（成员B）
//!   - persistence::Persistence（成员B）——追加日志与启动恢复
//!   - protocol 常量：DEFAULT_ADDR / DEFAULT_PORT / DEFAULT_DATA_FILE / MAX_MSG_LEN

use crate::parser;
use crate::persistence::{LogRecord, Persistence};
use crate::protocol::Command;
use crate::protocol::error;
use crate::protocol::{DEFAULT_DATA_FILE, DEFAULT_PORT};
use crate::store::KVStore;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// 服务器：封装内存存储与持久化，execute 为统一执行入口。
///
/// 第4天起以 `Arc<Mutex<Server>>` 形式被多个连接线程共享，
/// execute 内的数据操作（含写日志）天然在锁保护下串行执行。
pub struct Server {
    store: KVStore,
    persistence: Persistence,
    /// 服务器启动时刻（用于 STATUS 显示运行时长）
    started_at: std::time::Instant,
    /// 当前连接数（原子计数，由 handle_connection 进出时更新）
    connections: AtomicUsize,
    /// 累计处理的命令数（原子计数，每次 execute +1）
    commands: AtomicU64,
}

impl Server {
    /// 新建服务器（指定数据文件路径；数据恢复在 run 中通过 recover() 完成）
    pub fn new(data_file: &str) -> Self {
        Server {
            store: KVStore::new(),
            persistence: Persistence::new(data_file),
            started_at: std::time::Instant::now(),
            connections: AtomicUsize::new(0),
            commands: AtomicU64::new(0),
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
    /// 并发安全：本方法整体在调用方的 Mutex 锁内执行，写日志+更新内存不可分割。
    /// 所有错误转成 "ERROR <说明>" 文本返回，绝不 panic，保证循环继续。
    pub fn execute(&mut self, parsed: &parser::ParsedCommand) -> Option<String> {
        // 命令计数（STATUS 显示用）
        self.commands.fetch_add(1, Ordering::SeqCst);
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
            Command::Status => {
                let count = self.store.len();
                let connections = self.connections.load(Ordering::SeqCst);
                let uptime = self.started_at.elapsed().as_secs();
                let commands = self.commands.load(Ordering::SeqCst);
                format!(
                    "STATUS count={} connections={} uptime={}s commands={}",
                    count, connections, uptime, commands
                )
            }
            Command::Ping => "PONG".to_string(),
            Command::Exit => return None, // 收到 EXIT，退出服务器
        };
        Some(reply)
    }
}

/// 启动服务器。
/// 默认：网络模式；`--local`：本地 stdin 模式（第2天，调试/演示用）。
///
/// 网络模式参数（第4天）：
///   --port <端口>   监听端口（默认 7878）
///   --data <路径>   数据文件路径（默认 data/kv.log）
pub fn run(args: &[String]) {
    if args.iter().any(|a| a == "--local") {
        run_local();
        return;
    }
    let port = get_arg(args, "--port").unwrap_or_else(|| DEFAULT_PORT.to_string());
    let data_file = get_arg(args, "--data").unwrap_or_else(|| DEFAULT_DATA_FILE.to_string());
    let addr = format!("127.0.0.1:{}", port);
    run_network(&addr, &data_file);
}

/// 从命令行参数中取 `--name` 后面的值
fn get_arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
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

/// 第4天模式：TCP 网络服务（多客户端并发）
///
/// - 每接受一个连接就 `thread::spawn` 一个处理线程，互不阻塞；
/// - `Arc<Mutex<Server>>` 供所有连接线程共享同一份存储与持久化；
/// - 单个连接线程 panic/退出不影响服务器主循环和其他连接。
fn run_network(addr: &str, data_file: &str) {
    let listener = match TcpListener::bind(addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("监听 {} 失败: {}", addr, e);
            std::process::exit(1);
        }
    };

    // 启动恢复：数据文件损坏/格式异常时明确报错退出，绝不静默清空
    let server = Arc::new(Mutex::new(Server::new(data_file)));
    {
        let mut guard = server.lock().expect("服务器锁中毒");
        if let Err(e) = guard.recover() {
            eprintln!("启动失败: 数据文件异常 - {}", e);
            std::process::exit(1);
        }
    }

    println!("kvstore 服务器已启动（第4天：网络模式，多客户端并发）");
    println!("监听地址: {}", addr);
    println!("数据文件: {}", data_file);
    println!("启动状态: 正在监听，等待客户端连接...");
    println!();

    loop {
        match listener.accept() {
            Ok((stream, client_addr)) => {
                println!("[连接] {} 已连接", client_addr);
                let server = Arc::clone(&server);
                // 每连接一线程：连接处理完（EXIT/断开）线程结束，服务器继续 accept
                std::thread::spawn(move || {
                    handle_connection(stream, server);
                    println!("[连接] {} 已断开", client_addr);
                });
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
/// 并发安全核心：**锁只在执行数据操作（execute）时短暂持有**，
/// 等待网络输入（read_line）和发送响应（send_line）时都不持锁——
/// 因此多个客户端可以真正并行：一个客户端在等待输入时，
/// 其他客户端照常读写数据。
///
/// - BufReader::read_line 自动处理 TCP 消息分段（半包/粘包）；
/// - 超长请求由 parser 的 MAX_MSG_LEN 校验并回 ERROR；
/// - 客户端断开（read_line 返回 0）或收到 EXIT 时结束本连接；
/// - 单连接内一条命令出错不影响后续命令。
fn handle_connection(stream: TcpStream, server: Arc<Mutex<Server>>) {
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

    // 连接进入：连接数 +1（短暂加锁，仅更新原子计数）
    server
        .lock()
        .expect("服务器锁中毒")
        .connections
        .fetch_add(1, Ordering::SeqCst);

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

        // 解析；错误回写 ERROR 后继续处理下一条（不持锁）
        let parsed = match parser::parse_command(line) {
            Ok(p) => p,
            Err(e) => {
                if send_line(&mut writer, &format!("ERROR {}", e.message)).is_err() {
                    break;
                }
                continue;
            }
        };

        // 加锁执行——锁范围最小化：只覆盖 execute（数据操作+写日志）
        let reply = {
            let mut guard = server.lock().expect("服务器锁中毒");
            match guard.execute(&parsed) {
                Some(r) => r,
                None => break, // 收到 EXIT，关闭本连接
            }
        }; // 锁在此处释放

        if send_line(&mut writer, &reply).is_err() {
            break;
        }
    }

    // 连接离开：连接数 -1（所有 break 路径都会走到这里）
    server
        .lock()
        .expect("服务器锁中毒")
        .connections
        .fetch_sub(1, Ordering::SeqCst);
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
        let path =
            std::env::temp_dir().join(format!("kvstore_ut_{}_{}.log", std::process::id(), tag));
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
        // 新格式：STATUS count=N connections=M uptime=Ss commands=C
        assert!(reply.starts_with("STATUS count=0 "), "意外输出: {}", reply);
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
        let reply = server
            .execute(&cmd(Command::Set, Some("course"), Some("Rust")))
            .unwrap();
        assert_eq!(reply, "OK");
        // 写入后应能在内存中查到
        let reply = server
            .execute(&cmd(Command::Get, Some("course"), None))
            .unwrap();
        assert_eq!(reply, "VALUE course Rust");
    }

    #[test]
    fn get_missing_key_reports_error() {
        let mut server = make_server("get_missing");
        let reply = server
            .execute(&cmd(Command::Get, Some("nope"), None))
            .unwrap();
        assert_eq!(reply, format!("ERROR {}", error::KEY_NOT_FOUND));
    }

    #[test]
    fn del_missing_key_reports_error_without_log() {
        let mut server = make_server("del_missing");
        let reply = server
            .execute(&cmd(Command::Del, Some("nope"), None))
            .unwrap();
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
        let path =
            std::env::temp_dir().join(format!("kvstore_ut_{}_recover.log", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let mut server = Server::new(path.to_str().unwrap());
        server
            .execute(&cmd(Command::Set, Some("k1"), Some("v1")))
            .unwrap();
        server
            .execute(&cmd(Command::Set, Some("k2"), Some("v2")))
            .unwrap();
        drop(server); // 关闭（模拟服务器退出）

        // 重启：新 Server 实例 + recover
        let mut restarted = Server::new(path.to_str().unwrap());
        restarted.recover().unwrap();
        let reply = restarted
            .execute(&cmd(Command::Get, Some("k1"), None))
            .unwrap();
        assert_eq!(reply, "VALUE k1 v1");
        let reply = restarted
            .execute(&cmd(Command::Get, Some("k2"), None))
            .unwrap();
        assert_eq!(reply, "VALUE k2 v2");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn recover_replays_delete() {
        let path =
            std::env::temp_dir().join(format!("kvstore_ut_{}_recover_del.log", std::process::id()));
        let _ = std::fs::remove_file(&path);

        let mut server = Server::new(path.to_str().unwrap());
        server
            .execute(&cmd(Command::Set, Some("k"), Some("v")))
            .unwrap();
        server.execute(&cmd(Command::Del, Some("k"), None)).unwrap();
        drop(server);

        let mut restarted = Server::new(path.to_str().unwrap());
        restarted.recover().unwrap();
        let reply = restarted
            .execute(&cmd(Command::Get, Some("k"), None))
            .unwrap();
        assert_eq!(reply, format!("ERROR {}", error::KEY_NOT_FOUND));
        let _ = std::fs::remove_file(&path);
    }

    /// 第4天并发安全测试：多线程同时写入不同键，全部成功且互不覆盖
    #[test]
    fn concurrent_execute_is_safe() {
        let server = Arc::new(Mutex::new(make_server("concurrent")));
        let mut handles = Vec::new();
        for i in 0..8 {
            let server = Arc::clone(&server);
            handles.push(std::thread::spawn(move || {
                let mut guard = server.lock().unwrap();
                let reply = guard
                    .execute(&cmd(Command::Set, Some(&format!("key{}", i)), Some("v")))
                    .unwrap();
                assert_eq!(reply, "OK");
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        // 8 个键全部写入成功
        let mut guard = server.lock().unwrap();
        let reply = guard.execute(&cmd(Command::Status, None, None)).unwrap();
        assert!(reply.starts_with("STATUS count=8 "), "意外输出: {}", reply);
    }

    /// 第4天并发安全测试：多线程同时写同一键，最终值一定是某一次写入（不损坏、不 panic）
    #[test]
    fn concurrent_write_same_key_no_corruption() {
        let server = Arc::new(Mutex::new(make_server("concurrent_same")));
        let mut handles = Vec::new();
        for i in 0..16 {
            let server = Arc::clone(&server);
            handles.push(std::thread::spawn(move || {
                let mut guard = server.lock().unwrap();
                let reply = guard
                    .execute(&cmd(Command::Set, Some("shared"), Some(&format!("v{}", i))))
                    .unwrap();
                assert_eq!(reply, "OK");
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        // 最终值是 16 次写入中的某一次
        let mut guard = server.lock().unwrap();
        let reply = guard
            .execute(&cmd(Command::Get, Some("shared"), None))
            .unwrap();
        assert!(reply.starts_with("VALUE shared v"), "意外值: {}", reply);
    }
}
