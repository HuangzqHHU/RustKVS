//! 压力测试与 TTL 集成测试（成员D负责 · 冲刺拓展）
//!
//! 一、压力测试：大量并发连接下服务器不崩溃、数据不丢失
//! 二、TTL 集成测试：键过期、未过期、覆盖更新等场景（等B/C/A接口完成后启用）

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use kvstore::parser;
use kvstore::server::Server;

// ------------------------------------------------------------
// 辅助：启动并发测试服务器
// ------------------------------------------------------------

fn start_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let data_file = format!(
        "{}/kvstore_stress_test_{}.log",
        std::env::temp_dir().to_string_lossy(),
        std::process::id()
    );
    let _ = std::fs::remove_file(&data_file);

    let server = Arc::new(Mutex::new(Server::new(&data_file)));

    thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let server_clone = Arc::clone(&server);
                    thread::spawn(move || {
                        handle_conn(stream, server_clone);
                    });
                }
                Err(_) => break,
            }
        }
    });

    thread::sleep(Duration::from_millis(50));
    addr
}

fn handle_conn(stream: TcpStream, server: Arc<Mutex<Server>>) {
    let read_stream = stream.try_clone().unwrap();
    let mut reader = BufReader::new(read_stream);
    let mut writer = stream;

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => break,
        }
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }

        let parsed = match parser::parse_command(line) {
            Ok(p) => p,
            Err(e) => {
                let _ = writer.write_all(format!("ERROR {}\n", e.message).as_bytes());
                let _ = writer.flush();
                continue;
            }
        };

        let reply = {
            let mut guard = server.lock().expect("服务器锁中毒");
            match guard.execute(&parsed) {
                Some(r) => r,
                None => break,
            }
        };

        let _ = writer.write_all(reply.as_bytes());
        let _ = writer.write_all(b"\n");
        let _ = writer.flush();
    }
}

fn connect(addr: &str) -> (BufReader<TcpStream>, TcpStream) {
    let stream = TcpStream::connect(addr).unwrap();
    let reader = BufReader::new(stream.try_clone().unwrap());
    let writer = stream;
    (reader, writer)
}

fn send_cmd(writer: &mut TcpStream, reader: &mut BufReader<TcpStream>, cmd: &str) -> String {
    writer.write_all(cmd.as_bytes()).unwrap();
    writer.write_all(b"\n").unwrap();
    writer.flush().unwrap();

    let mut resp = String::new();
    reader.read_line(&mut resp).unwrap();
    resp.trim_end().to_string()
}

// ============================================================
// 一、压力测试
// ============================================================

/// 100 并发连接，每个连接执行 10 次 SET（不同键），共 1000 次操作
/// 验证：全部成功、总数正确、无崩溃
#[test]
fn stress_100_clients_1000_ops() {
    let addr = start_server();
    let mut handles = vec![];

    let start = Instant::now();

    for client_id in 0..100 {
        let addr = addr.clone();
        let handle = thread::spawn(move || {
            let (mut reader, mut writer) = connect(&addr);
            for i in 0..10 {
                let key = format!("stress_{}_{}", client_id, i);
                let value = format!("val_{}_{}", client_id, i);
                assert_eq!(
                    send_cmd(&mut writer, &mut reader, &format!("SET {} {}", key, value)),
                    "OK"
                );
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }

    let elapsed = start.elapsed();

    // 验证总数
    let (mut reader, mut writer) = connect(&addr);
    let status = send_cmd(&mut writer, &mut reader, "STATUS");
    assert!(status.starts_with("STATUS count="), "STATUS 格式不对: {}", status);

    // 100 客户端 × 10 键 = 1000 个
    let count_str = status.trim_start_matches("STATUS count=");
    let count: usize = count_str
        .split_whitespace()
        .next()
        .unwrap_or("0")
        .parse()
        .unwrap_or(0);
    assert_eq!(count, 1000, "应写入 1000 个键，实际: {}", count);

    // 打印耗时（方便看性能）
    println!("[压力测试] 100 并发 × 10 操作 = 1000 次，耗时: {:?}", elapsed);
}

/// 50 并发混合读写（SET + GET + DEL + PING），不崩溃
#[test]
fn stress_50_clients_mixed_ops() {
    let addr = start_server();

    // 先写入初始数据
    {
        let (mut reader, mut writer) = connect(&addr);
        for i in 0..50 {
            send_cmd(&mut writer, &mut reader, &format!("SET init_{} val{}", i, i));
        }
    }

    let mut handles = vec![];

    for client_id in 0..50 {
        let addr = addr.clone();
        let handle = thread::spawn(move || {
            let (mut reader, mut writer) = connect(&addr);
            for i in 0..20 {
                match i % 4 {
                    0 => {
                        let key = format!("m{}_{}", client_id, i);
                        assert_eq!(
                            send_cmd(&mut writer, &mut reader, &format!("SET {} v", key)),
                            "OK"
                        );
                    }
                    1 => {
                        let key = format!("init_{}", (client_id + i) % 50);
                        let resp = send_cmd(&mut writer, &mut reader, &format!("GET {}", key));
                        assert!(resp.starts_with("VALUE ") || resp.starts_with("ERROR"));
                    }
                    2 => {
                        assert_eq!(send_cmd(&mut writer, &mut reader, "PING"), "PONG");
                    }
                    _ => {
                        let resp = send_cmd(&mut writer, &mut reader, "LIST");
                        assert!(resp.starts_with("KEYS"));
                    }
                }
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }

    let (mut reader, mut writer) = connect(&addr);
    assert_eq!(send_cmd(&mut writer, &mut reader, "PING"), "PONG");
}

/// 高并发下删除同一键，不崩溃、最终一致
#[test]
fn stress_concurrent_delete_hotkey() {
    let addr = start_server();

    {
        let (mut reader, mut writer) = connect(&addr);
        send_cmd(&mut writer, &mut reader, "SET hotkey value");
    }

    let mut handles = vec![];

    for _ in 0..100 {
        let addr = addr.clone();
        let handle = thread::spawn(move || {
            let (mut reader, mut writer) = connect(&addr);
            let resp = send_cmd(&mut writer, &mut reader, "DEL hotkey");
            assert!(resp == "OK" || resp.starts_with("ERROR"), "DEL 响应异常: {}", resp);
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }

    let (mut reader, mut writer) = connect(&addr);
    let resp = send_cmd(&mut writer, &mut reader, "GET hotkey");
    assert!(resp.starts_with("ERROR"), "热键最终应不存在，实际: {}", resp);
}

/// 大量并发 PING，验证服务器吞吐
#[test]
fn stress_many_pings() {
    let addr = start_server();
    let mut handles = vec![];

    let start = Instant::now();

    for _ in 0..20 {
        let addr = addr.clone();
        let handle = thread::spawn(move || {
            let (mut reader, mut writer) = connect(&addr);
            for _ in 0..100 {
                assert_eq!(send_cmd(&mut writer, &mut reader, "PING"), "PONG");
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }

    let elapsed = start.elapsed();
    println!("[压力测试] 20 并发 × 100 PING = 2000 次，耗时: {:?}", elapsed);
}

// ============================================================
// 二、TTL 集成测试（等 B/C/A 接口完成后取消 ignore）
// ============================================================

/// SET 带 ttl，未过期时 GET 能查到
#[test]
fn ttl_not_expired_get_works() {
    let addr = start_server();
    let (mut reader, mut writer) = connect(&addr);

    assert_eq!(send_cmd(&mut writer, &mut reader, "SET k v 10"), "OK");
    assert_eq!(send_cmd(&mut writer, &mut reader, "GET k"), "VALUE k v");
}

/// SET 带 ttl，过期后 GET 返回键不存在
#[test]
fn ttl_expired_get_returns_not_found() {
    let addr = start_server();
    let (mut reader, mut writer) = connect(&addr);

    assert_eq!(send_cmd(&mut writer, &mut reader, "SET k v 1"), "OK");
    assert_eq!(send_cmd(&mut writer, &mut reader, "GET k"), "VALUE k v");

    thread::sleep(Duration::from_secs(2));

    let resp = send_cmd(&mut writer, &mut reader, "GET k");
    assert!(resp.starts_with("ERROR"), "过期后应返回 ERROR，实际: {}", resp);
}

/// 不带 ttl 的 SET 覆盖带 ttl 的键 → 变成永不过期
#[test]
fn ttl_overwrite_without_ttl_makes_permanent() {
    let addr = start_server();
    let (mut reader, mut writer) = connect(&addr);

    assert_eq!(send_cmd(&mut writer, &mut reader, "SET k v1 1"), "OK");
    assert_eq!(send_cmd(&mut writer, &mut reader, "SET k v2"), "OK");

    thread::sleep(Duration::from_secs(2));

    assert_eq!(send_cmd(&mut writer, &mut reader, "GET k"), "VALUE k v2");
}

/// 带 ttl 的 SET 覆盖不带 ttl 的键 → 新 ttl 生效
#[test]
fn ttl_overwrite_with_new_ttl() {
    let addr = start_server();
    let (mut reader, mut writer) = connect(&addr);

    assert_eq!(send_cmd(&mut writer, &mut reader, "SET k v1"), "OK");
    assert_eq!(send_cmd(&mut writer, &mut reader, "SET k v2 1"), "OK");

    thread::sleep(Duration::from_secs(2));

    let resp = send_cmd(&mut writer, &mut reader, "GET k");
    assert!(resp.starts_with("ERROR"), "过期后应返回 ERROR，实际: {}", resp);
}

/// 过期键在 LIST 中不出现
#[test]
fn ttl_expired_not_in_list() {
    let addr = start_server();
    let (mut reader, mut writer) = connect(&addr);

    assert_eq!(send_cmd(&mut writer, &mut reader, "SET a 1 1"), "OK");
    assert_eq!(send_cmd(&mut writer, &mut reader, "SET b 2"), "OK");

    thread::sleep(Duration::from_secs(2));

    let resp = send_cmd(&mut writer, &mut reader, "LIST");
    assert_eq!(resp, "KEYS b");
}

/// 过期键不计入 STATUS count
#[test]
fn ttl_expired_not_in_status() {
    let addr = start_server();
    let (mut reader, mut writer) = connect(&addr);

    assert_eq!(send_cmd(&mut writer, &mut reader, "SET a 1 1"), "OK");
    assert_eq!(send_cmd(&mut writer, &mut reader, "SET b 2"), "OK");

    let status_before = send_cmd(&mut writer, &mut reader, "STATUS");
    assert!(status_before.contains("count=2"));

    thread::sleep(Duration::from_secs(2));

    let status_after = send_cmd(&mut writer, &mut reader, "STATUS");
    assert!(status_after.contains("count=1"), "过期后 count 应为 1，实际: {}", status_after);
}

/// ttl 为 0 → 解析错误
#[test]
fn ttl_zero_is_error() {
    let addr = start_server();
    let (mut reader, mut writer) = connect(&addr);

    let resp = send_cmd(&mut writer, &mut reader, "SET k v 0");
    assert!(resp.starts_with("ERROR"), "ttl=0 应返回 ERROR，实际: {}", resp);
}

/// ttl 非数字 → 解析错误
#[test]
fn ttl_negative_is_error() {
    let addr = start_server();
    let (mut reader, mut writer) = connect(&addr);

    let resp = send_cmd(&mut writer, &mut reader, "SET k v -5");
    assert!(resp.starts_with("ERROR"), "负数 ttl 应返回 ERROR，实际: {}", resp);
}

/// DEL 过期键 → 键不存在
#[test]
fn ttl_expired_del_returns_not_found() {
    let addr = start_server();
    let (mut reader, mut writer) = connect(&addr);

    assert_eq!(send_cmd(&mut writer, &mut reader, "SET k v 1"), "OK");
    thread::sleep(Duration::from_secs(2));

    let resp = send_cmd(&mut writer, &mut reader, "DEL k");
    assert!(resp.starts_with("ERROR"), "过期键 DEL 应返回键不存在，实际: {}", resp);
}