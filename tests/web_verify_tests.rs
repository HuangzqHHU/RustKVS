//! Web 管理界面——成员B 验证测试（互通 / 恢复 / 并发）
//!
//! 依据 `WEB-PLAN.md` 第三节"成员B"任务：
//!   ① Web 与 TCP 数据互通验证（网页 SET → 客户端 GET，反向同理）
//!   ② 网页操作后重启恢复验证
//!   ③ 并发下 Web 线程锁安全复核（Web 与 TCP 线程同时读写同一 Server）
//!
//! 测试方式与生产 wiring（server.rs::run_network）一致：
//!   - Web 线程：调用真实的 `web::spawn_web_server`（内部加锁调 webpage::handle）；
//!   - TCP 线程：accept 循环复刻 server.rs::handle_connection 语义（parse → execute）；
//!   - 两者共享同一个 `Arc<Mutex<Server>>`；
//!   - 测试用 std TcpStream 手写 HTTP 请求与协议命令，不依赖浏览器。

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use kvstore::parser;
use kvstore::protocol::error;
use kvstore::server::Server;

// ------------------------------------------------------------
// 辅助：一台同时监听 TCP 命令 + Web 管理端口的"测试服务器"
// ------------------------------------------------------------

/// 测试用服务器：web 线程与 TCP 处理线程共享同一 Server 实例
struct TestServer {
    tcp_addr: String,
    web_addr: String,
    #[allow(dead_code)]
    server: Arc<Mutex<Server>>, // 持有强引用，模拟"运行中"的服务器
}

/// 每个测试独立的临时数据文件（避免并行测试互相干扰）
fn temp_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "kvstore_webv_{}_{}.log",
        std::process::id(),
        tag
    ))
}

/// 启动测试服务器：绑定两个随机端口，恢复数据（缺失文件视为首次启动）
fn start_server(data_file: &Path) -> TestServer {
    let tcp_listener = TcpListener::bind("127.0.0.1:0").expect("绑定 TCP 端口失败");
    let tcp_addr = tcp_listener.local_addr().unwrap().to_string();
    let web_listener = TcpListener::bind("127.0.0.1:0").expect("绑定 Web 端口失败");
    let web_addr = web_listener.local_addr().unwrap().to_string();

    // 启动恢复（与 run_network 一致）
    let mut srv = Server::new(data_file.to_str().unwrap());
    srv.recover().expect("测试服务器恢复数据失败");
    let server = Arc::new(Mutex::new(srv));

    // Web 线程：真实 spawn_web_server（内部每请求加锁调 webpage::handle）
    kvstore::web::spawn_web_server(web_listener, Arc::clone(&server));

    // TCP 处理线程：accept 循环，每连接一线程（复刻 server.rs handle_connection 语义）
    let tcp_server = Arc::clone(&server);
    thread::spawn(move || {
        for stream in tcp_listener.incoming() {
            let Ok(stream) = stream else { continue };
            let s = Arc::clone(&tcp_server);
            thread::spawn(move || handle_tcp_conn(stream, s));
        }
    });

    // 给监听线程一点启动时间
    thread::sleep(Duration::from_millis(80));

    TestServer {
        tcp_addr,
        web_addr,
        server,
    }
}

/// 处理一条 TCP 连接：逐行读命令 → 解析 → 加锁 execute → 写回（复刻 server.rs）
fn handle_tcp_conn(mut stream: TcpStream, server: Arc<Mutex<Server>>) {
    let read_stream = stream.try_clone().expect("克隆 TCP 连接失败");
    let mut reader = BufReader::new(read_stream);

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break, // 客户端断开
            Ok(_) => {}
        }
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }

        let parsed = match parser::parse_command(line) {
            Ok(p) => p,
            Err(e) => {
                let _ = stream.write_all(format!("ERROR {}\n", e.message).as_bytes());
                let _ = stream.flush();
                continue;
            }
        };
        let reply = {
            let mut guard = server.lock().expect("服务器锁中毒");
            match guard.execute(&parsed) {
                Some(r) => r,
                None => break, // EXIT → 关闭本连接，服务器继续
            }
        };
        if stream
            .write_all(reply.as_bytes())
            .and_then(|_| stream.write_all(b"\n"))
            .and_then(|_| stream.flush())
            .is_err()
        {
            break;
        }
    }
}

// ------------------------------------------------------------
// HTTP 客户端（std TcpStream 手写请求）
// ------------------------------------------------------------

/// 表单字段 URL 编码：空格 → '+'，非 ASCII → %XX
fn form_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b' ' => out.push('+'),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

/// 发送原始 HTTP 请求并读完整响应（Connection: close → 读到 EOF）
fn http_raw(addr: &str, raw: &str) -> String {
    let mut stream = TcpStream::connect(addr).expect("连接 Web 端口失败");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream.write_all(raw.as_bytes()).expect("发送 HTTP 请求失败");
    let mut resp = String::new();
    stream
        .read_to_string(&mut resp)
        .expect("读取 HTTP 响应失败");
    resp
}

fn http_get(addr: &str, path: &str) -> String {
    http_raw(
        addr,
        &format!(
            "GET {} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
            path
        ),
    )
}

/// POST 表单：command=整条命令文本（如 "SET course Rust"）
fn http_post(addr: &str, path: &str, command: &str) -> String {
    let body = format!("command={}", form_encode(command));
    http_raw(
        addr,
        &format!(
            "POST {} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            path,
            body.len(),
            body
        ),
    )
}

fn http_status(resp: &str) -> u16 {
    resp.lines()
        .next()
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

fn http_body(resp: &str) -> &str {
    resp.split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or(resp)
}

// ------------------------------------------------------------
// TCP 协议客户端（一个命令一条连接）
// ------------------------------------------------------------

fn tcp_cmd(addr: &str, cmd: &str) -> String {
    let stream = TcpStream::connect(addr).expect("连接 TCP 端口失败");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = stream;
    writer.write_all(cmd.as_bytes()).unwrap();
    writer.write_all(b"\n").unwrap();
    writer.flush().unwrap();
    let mut resp = String::new();
    reader.read_line(&mut resp).expect("读取 TCP 响应失败");
    resp.trim_end().to_string()
}

// ============================================================
// ① Web 与 TCP 数据互通验证
// ============================================================

#[test]
fn web_and_tcp_share_data_bidirectionally() {
    let f = temp_path("interop");
    let _ = std::fs::remove_file(&f);
    let s = start_server(&f);

    // 1) 网页 SET（无 TTL，永久）→ TCP 客户端能查到
    let resp = http_post(&s.web_addr, "/cmd", "SET course Rust");
    assert_eq!(http_status(&resp), 200);
    assert!(
        http_body(&resp).contains("OK"),
        "网页 SET 未返回 OK: {}",
        http_body(&resp)
    );
    assert_eq!(tcp_cmd(&s.tcp_addr, "GET course"), "VALUE course Rust");

    // 2) 网页 SET 中文值 → TCP 能查到（UTF-8 全链路互通）
    let resp = http_post(&s.web_addr, "/cmd", "SET greeting 你好世界");
    assert!(http_body(&resp).contains("OK"), "中文写入失败");
    assert_eq!(
        tcp_cmd(&s.tcp_addr, "GET greeting"),
        "VALUE greeting 你好世界"
    );

    // 3) 反向互通：TCP 客户端 SET → 网页 /get 能查到
    assert_eq!(tcp_cmd(&s.tcp_addr, "SET note hello"), "OK");
    let resp = http_get(&s.web_addr, "/get?key=note");
    assert_eq!(http_status(&resp), 200);
    assert!(
        http_body(&resp).contains("VALUE note hello"),
        "网页查不到 TCP 写入: {}",
        http_body(&resp)
    );

    // 4) 主页数据表同时包含两边写入的键（同一份数据）
    let page = http_get(&s.web_addr, "/");
    let body = http_body(&page);
    assert!(body.contains("<td>course</td>"), "主页缺 course: {}", body);
    assert!(body.contains("<td>note</td>"), "主页缺 note: {}", body);

    // 5) 网页 DEL → TCP 查询键不存在（双向删除）
    let resp = http_post(&s.web_addr, "/cmd", "DEL course");
    assert!(http_body(&resp).contains("OK"), "网页 DEL 失败");
    assert_eq!(
        tcp_cmd(&s.tcp_addr, "GET course"),
        format!("ERROR {}", error::KEY_NOT_FOUND)
    );

    // 6) 两侧视角一致：剩余键数相同（note + greeting = 2）
    assert!(
        tcp_cmd(&s.tcp_addr, "STATUS").starts_with("STATUS count=2 "),
        "互通后数据数量不符"
    );
}

// ============================================================
// ② 网页操作后重启恢复验证
// ============================================================

#[test]
fn web_writes_survive_restart() {
    let f = temp_path("recover");
    let _ = std::fs::remove_file(&f);

    // 第一代服务器：全部操作都走网页（HTTP），随后"关闭"
    {
        let s1 = start_server(&f);
        let resp = http_post(&s1.web_addr, "/cmd", "SET durable keepme");
        assert!(http_body(&resp).contains("OK"), "网页 SET durable 失败");
        let resp = http_post(&s1.web_addr, "/cmd", "SET gone bye");
        assert!(http_body(&resp).contains("OK"));
        let resp = http_post(&s1.web_addr, "/cmd", "DEL gone");
        assert!(http_body(&resp).contains("OK"));
        // s1 作用域结束即 drop（模拟服务器关闭）
    }

    // 第二代：同一数据文件"重启"，启动时 recover
    let s2 = start_server(&f);

    // 网页写入的键恢复；DEL 也被日志重放
    assert_eq!(
        tcp_cmd(&s2.tcp_addr, "GET durable"),
        "VALUE durable keepme"
    );
    assert_eq!(
        tcp_cmd(&s2.tcp_addr, "GET gone"),
        format!("ERROR {}", error::KEY_NOT_FOUND)
    );

    // 网页视角一致：重启后主页显示 durable、不含 gone
    let page = http_get(&s2.web_addr, "/");
    let body = http_body(&page);
    assert!(
        body.contains("<td>durable</td>"),
        "重启后主页缺 durable: {}",
        body
    );
    assert!(
        !body.contains("<td>gone</td>"),
        "重启后 gone 不应存在: {}",
        body
    );
}

// ============================================================
// ③ 并发下 Web 线程锁安全复核（Web 与 TCP 线程同时读写）
// ============================================================

#[test]
fn concurrent_web_and_tcp_access_lock_safe() {
    let f = temp_path("concurrent");
    let _ = std::fs::remove_file(&f);
    let s = start_server(&f);

    const WEB_N: usize = 8;
    const TCP_N: usize = 8;

    let web_addr = s.web_addr.clone();
    let tcp_addr = s.tcp_addr.clone();
    let mut handles = Vec::new();

    // 一半线程走网页（真实 HTTP）写不同键
    for i in 0..WEB_N {
        let a = web_addr.clone();
        handles.push(thread::spawn(move || {
            let resp = http_post(&a, "/cmd", &format!("SET wkey{} wval{}", i, i));
            assert!(http_body(&resp).contains("OK"), "并发网页写入失败");
        }));
    }
    // 一半线程走 TCP 客户端写不同键
    for i in 0..TCP_N {
        let a = tcp_addr.clone();
        handles.push(thread::spawn(move || {
            assert_eq!(tcp_cmd(&a, &format!("SET tkey{} tval{}", i, i)), "OK");
        }));
    }
    for h in handles {
        h.join().expect("并发线程 panic（锁安全失败）");
    }

    // 16 个键全部写入成功、互不丢失、无损坏（经 TCP 视角）
    let list = tcp_cmd(&s.tcp_addr, "LIST");
    for i in 0..WEB_N {
        assert!(
            list.contains(&format!("wkey{}", i)),
            "缺少网页键 wkey{}: {}",
            i,
            list
        );
    }
    for i in 0..TCP_N {
        assert!(
            list.contains(&format!("tkey{}", i)),
            "缺少 TCP 键 tkey{}: {}",
            i,
            list
        );
    }
    assert!(
        tcp_cmd(&s.tcp_addr, "STATUS").starts_with(&format!("STATUS count={} ", WEB_N + TCP_N)),
        "并发写入数量不符"
    );

    // 网页视角同样一致
    let page = http_get(&s.web_addr, "/");
    let body = http_body(&page);
    assert!(
        body.contains("<td>wkey0</td>")
            && body.contains(&format!("<td>tkey{}</td>", TCP_N - 1)),
        "并发后网页表格与 TCP 不一致"
    );
}

// ============================================================
// 附带复核：网页 EXIT 被拒绝，共享 Server 不被退出
// ============================================================

#[test]
fn web_exit_rejected_and_server_alive() {
    let f = temp_path("exit");
    let _ = std::fs::remove_file(&f);
    let s = start_server(&f);

    // 网页提交 EXIT：execute 返回 None → 转错误提示，服务器不退出
    let resp = http_post(&s.web_addr, "/cmd", "EXIT");
    let body = http_body(&resp);
    assert!(
        body.contains("不允许在网页上执行"),
        "EXIT 未被拒绝: {}",
        body
    );

    // TCP 客户端依然可用 → 共享 Server 没被退出
    assert_eq!(tcp_cmd(&s.tcp_addr, "PING"), "PONG");
    assert_eq!(tcp_cmd(&s.tcp_addr, "SET alive yes"), "OK");
}
