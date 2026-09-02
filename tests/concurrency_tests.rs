//! 并发集成测试（成员D负责 · 第4天）
//!
//! 验证多客户端并发连接时：
//!   - 数据不丢失、不错乱
//!   - 服务器不崩溃
//!   - 最终状态一致

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use kvstore::parser;
use kvstore::server::Server;
use kvstore::protocol::error;

// ------------------------------------------------------------
// 辅助：启动并发测试服务器（使用 Server 结构体）
// ------------------------------------------------------------

fn start_concurrent_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    // 用临时数据文件，避免测试间互相干扰
    let data_file = format!(
        "{}/kvstore_concurrent_test_{}.log",
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

        // 加锁执行 Server::execute
        let reply = {
            let mut guard = server.lock().expect("服务器锁中毒");
            match guard.execute(&parsed) {
                Some(r) => r,
                None => break, // EXIT
            }
        }; // 锁释放

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
// 一、基础并发：多客户端同时写入不同键
// ============================================================

#[test]
fn concurrent_multi_client_write_different_keys() {
    let addr = start_concurrent_server();
    let mut handles = vec![];

    for client_id in 0..10 {
        let addr = addr.clone();
        let handle = thread::spawn(move || {
            let (mut reader, mut writer) = connect(&addr);
            for i in 0..10 {
                let key = format!("c{}_{}", client_id, i);
                let value = format!("val{}", client_id * 10 + i);
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

    let (mut reader, mut writer) = connect(&addr);
    assert!(send_cmd(&mut writer, &mut reader, "STATUS").starts_with("STATUS count=100 "), "STATUS 输出不符预期");
}

#[test]
fn concurrent_many_ping_clients() {
    let addr = start_concurrent_server();
    let mut handles = vec![];

    for _ in 0..20 {
        let addr = addr.clone();
        let handle = thread::spawn(move || {
            let (mut reader, mut writer) = connect(&addr);
            for _ in 0..5 {
                assert_eq!(send_cmd(&mut writer, &mut reader, "PING"), "PONG");
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }
}

// ============================================================
// 二、并发写同一键
// ============================================================

#[test]
fn concurrent_write_same_key_no_panic() {
    let addr = start_concurrent_server();
    let mut handles = vec![];

    for i in 0..50 {
        let addr = addr.clone();
        let value = format!("client_{}", i);
        let handle = thread::spawn(move || {
            let (mut reader, mut writer) = connect(&addr);
            for _ in 0..10 {
                assert_eq!(
                    send_cmd(&mut writer, &mut reader, &format!("SET shared_key {}", value)),
                    "OK"
                );
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }

    let (mut reader, mut writer) = connect(&addr);
    let resp = send_cmd(&mut writer, &mut reader, "GET shared_key");
    assert!(resp.starts_with("VALUE shared_key "), "应返回 VALUE，实际: {}", resp);
    assert!(send_cmd(&mut writer, &mut reader, "STATUS").starts_with("STATUS count=1 "), "STATUS 输出不符预期");
}

// ============================================================
// 三、并发读写混合
// ============================================================

#[test]
fn concurrent_read_write_mixed() {
    let addr = start_concurrent_server();

    {
        let (mut reader, mut writer) = connect(&addr);
        for i in 0..20 {
            send_cmd(&mut writer, &mut reader, &format!("SET key{} value{}", i, i));
        }
    }

    let mut handles = vec![];

    for i in 0..10 {
        let addr = addr.clone();
        let handle = thread::spawn(move || {
            let (mut reader, mut writer) = connect(&addr);
            for j in 0..20 {
                let key = format!("w{}_{}", i, j);
                assert_eq!(
                    send_cmd(&mut writer, &mut reader, &format!("SET {} val", key)),
                    "OK"
                );
            }
        });
        handles.push(handle);
    }

    for _ in 0..10 {
        let addr = addr.clone();
        let handle = thread::spawn(move || {
            let (mut reader, mut writer) = connect(&addr);
            for j in 0..20 {
                let key = format!("key{}", j);
                let resp = send_cmd(&mut writer, &mut reader, &format!("GET {}", key));
                assert!(
                    resp.starts_with("VALUE ") || resp.starts_with("ERROR"),
                    "响应格式不对: {}", resp
                );
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

// ============================================================
// 四、并发删除
// ============================================================

#[test]
fn concurrent_delete_same_key() {
    let addr = start_concurrent_server();

    {
        let (mut reader, mut writer) = connect(&addr);
        send_cmd(&mut writer, &mut reader, "SET target value");
    }

    let mut handles = vec![];
    for _ in 0..30 {
        let addr = addr.clone();
        let handle = thread::spawn(move || {
            let (mut reader, mut writer) = connect(&addr);
            let resp = send_cmd(&mut writer, &mut reader, "DEL target");
            assert!(
                resp == "OK" || resp.starts_with("ERROR"),
                "DEL 响应不对: {}", resp
            );
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }

    let (mut reader, mut writer) = connect(&addr);
    let resp = send_cmd(&mut writer, &mut reader, "GET target");
    assert_eq!(resp, format!("ERROR {}", error::KEY_NOT_FOUND));
}

// ============================================================
// 五、计数器（验证互斥锁基本工作）
// ============================================================

#[test]
fn concurrent_counter_no_crash() {
    let addr = start_concurrent_server();

    {
        let (mut reader, mut writer) = connect(&addr);
        send_cmd(&mut writer, &mut reader, "SET counter 0");
    }

    let mut handles = vec![];
    for _ in 0..10 {
        let addr = addr.clone();
        let handle = thread::spawn(move || {
            let (mut reader, mut writer) = connect(&addr);
            for _ in 0..100 {
                let resp = send_cmd(&mut writer, &mut reader, "GET counter");
                if resp.starts_with("VALUE counter ") {
                    let val: i32 = resp.trim_start_matches("VALUE counter ").parse().unwrap_or(0);
                    send_cmd(&mut writer, &mut reader, &format!("SET counter {}", val + 1));
                }
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }

    let (mut reader, mut writer) = connect(&addr);
    let resp = send_cmd(&mut writer, &mut reader, "GET counter");
    assert!(resp.starts_with("VALUE counter "), "响应格式不对: {}", resp);

    let final_val: i32 = resp
        .trim_start_matches("VALUE counter ")
        .parse()
        .expect("计数器值应该是数字");
    assert!(final_val >= 0, "计数器不应为负");
    assert!(final_val <= 1000, "计数器不应超过 1000");
}

// ============================================================
// 六、大量并发连接压力测试
// ============================================================

#[test]
fn concurrent_50_clients_stress() {
    let addr = start_concurrent_server();
    let mut handles = vec![];

    for client_id in 0..50 {
        let addr = addr.clone();
        let handle = thread::spawn(move || {
            let (mut reader, mut writer) = connect(&addr);

            assert_eq!(send_cmd(&mut writer, &mut reader, "PING"), "PONG");

            let key = format!("stress_{}", client_id);
            assert_eq!(
                send_cmd(&mut writer, &mut reader, &format!("SET {} data", key)),
                "OK"
            );
            assert_eq!(
                send_cmd(&mut writer, &mut reader, &format!("GET {}", key)),
                format!("VALUE {} data", key)
            );
            assert_eq!(send_cmd(&mut writer, &mut reader, &format!("DEL {}", key)), "OK");

            let status = send_cmd(&mut writer, &mut reader, "STATUS");
            assert!(status.starts_with("STATUS count="));
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }

    let (mut reader, mut writer) = connect(&addr);
    assert!(send_cmd(&mut writer, &mut reader, "STATUS").starts_with("STATUS count=0 "), "STATUS 输出不符预期");
}

// ============================================================
// 七、并发 + LIST 一致性
// ============================================================

#[test]
fn concurrent_list_consistent() {
    let addr = start_concurrent_server();
    let mut handles = vec![];

    for i in 0..10 {
        let addr = addr.clone();
        let handle = thread::spawn(move || {
            let (mut reader, mut writer) = connect(&addr);
            for j in 0..20 {
                let key = format!("lst_{}_{}", i, j);
                send_cmd(&mut writer, &mut reader, &format!("SET {} v", key));
            }
        });
        handles.push(handle);
    }

    for _ in 0..5 {
        let addr = addr.clone();
        let handle = thread::spawn(move || {
            let (mut reader, mut writer) = connect(&addr);
            for _ in 0..20 {
                let resp = send_cmd(&mut writer, &mut reader, "LIST");
                assert!(resp.starts_with("KEYS"), "LIST 响应格式不对: {}", resp);
                if resp == "KEYS" {
                    continue;
                }
                let keys: Vec<&str> = resp.trim_start_matches("KEYS ").split(' ').collect();
                assert!(keys.len() <= 200, "键数量异常: {}", keys.len());
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }

    let (mut reader, mut writer) = connect(&addr);
    assert!(send_cmd(&mut writer, &mut reader, "STATUS").starts_with("STATUS count=200 "), "STATUS 输出不符预期");
}