//! 网络模式集成测试（成员D负责 · 第3天）
//!
//! 测试思路：
//!   1. 用 TcpListener 绑定端口 0（系统分配空闲端口）
//!   2. 子线程中启动服务器（复用 server 模块的 execute 逻辑）
//!   3. 主线程用 TcpStream 连接，逐行发命令、读响应
//!   4. 断言响应是否符合协议
//!
//! 共 20 个测试，覆盖：基础命令、增删改查、会话保持、
//! 错误隔离、EXIT/断开、特殊值、大小写、批量操作等。

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

use kvstore::parser;
use kvstore::protocol::error;
use kvstore::server::Server;

// ------------------------------------------------------------
// 辅助：在随机端口启动一个测试服务器，返回地址
// ------------------------------------------------------------

fn start_test_server() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    // 每个测试独立的临时数据文件（按端口区分），避免测试间互相干扰
    let data_file = std::env::temp_dir().join(format!("kvstore_net_{}.log", addr.port()));

    thread::spawn(move || {
        if let Ok((stream, _)) = listener.accept() {
            handle_conn(stream, &data_file);
        }
    });

    thread::sleep(Duration::from_millis(50));
    addr.to_string()
}

fn handle_conn(stream: TcpStream, data_file: &std::path::Path) {
    let read_stream = stream.try_clone().unwrap();
    let mut reader = BufReader::new(read_stream);
    let mut writer = stream;
    let mut server = Server::new(data_file.to_str().unwrap());

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

        match server.execute(&parsed) {
            Some(reply) => {
                let _ = writer.write_all(reply.as_bytes());
                let _ = writer.write_all(b"\n");
                let _ = writer.flush();
            }
            None => break,
        }
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
// 一、基础连接与简单命令
// ============================================================

#[test]
fn net_ping_pong() {
    let addr = start_test_server();
    let (mut reader, mut writer) = connect(&addr);
    assert_eq!(send_cmd(&mut writer, &mut reader, "PING"), "PONG");
}

#[test]
fn net_status_empty() {
    let addr = start_test_server();
    let (mut reader, mut writer) = connect(&addr);
    assert_eq!(
        send_cmd(&mut writer, &mut reader, "STATUS"),
        "STATUS count=0"
    );
}

#[test]
fn net_list_empty() {
    let addr = start_test_server();
    let (mut reader, mut writer) = connect(&addr);
    assert_eq!(send_cmd(&mut writer, &mut reader, "LIST"), "KEYS");
}

// ============================================================
// 二、SET / GET / DEL 基本操作
// ============================================================

#[test]
fn net_set_and_get() {
    let addr = start_test_server();
    let (mut reader, mut writer) = connect(&addr);

    assert_eq!(send_cmd(&mut writer, &mut reader, "SET name Alice"), "OK");
    assert_eq!(
        send_cmd(&mut writer, &mut reader, "GET name"),
        "VALUE name Alice"
    );
}

#[test]
fn net_get_missing() {
    let addr = start_test_server();
    let (mut reader, mut writer) = connect(&addr);
    let resp = send_cmd(&mut writer, &mut reader, "GET nope");
    assert_eq!(resp, format!("ERROR {}", error::KEY_NOT_FOUND));
}

#[test]
fn net_del_existing() {
    let addr = start_test_server();
    let (mut reader, mut writer) = connect(&addr);

    let _ = send_cmd(&mut writer, &mut reader, "SET k v");
    assert_eq!(send_cmd(&mut writer, &mut reader, "DEL k"), "OK");
    assert_eq!(
        send_cmd(&mut writer, &mut reader, "GET k"),
        format!("ERROR {}", error::KEY_NOT_FOUND)
    );
}

#[test]
fn net_del_missing() {
    let addr = start_test_server();
    let (mut reader, mut writer) = connect(&addr);
    let resp = send_cmd(&mut writer, &mut reader, "DEL nope");
    assert_eq!(resp, format!("ERROR {}", error::KEY_NOT_FOUND));
}

// ============================================================
// 三、同连接多命令（会话保持）
// ============================================================

#[test]
fn net_session_persistence() {
    let addr = start_test_server();
    let (mut reader, mut writer) = connect(&addr);

    assert_eq!(send_cmd(&mut writer, &mut reader, "SET a 1"), "OK");
    assert_eq!(send_cmd(&mut writer, &mut reader, "SET b 2"), "OK");
    assert_eq!(send_cmd(&mut writer, &mut reader, "SET c 3"), "OK");
    assert_eq!(
        send_cmd(&mut writer, &mut reader, "STATUS"),
        "STATUS count=3"
    );
    assert_eq!(send_cmd(&mut writer, &mut reader, "LIST"), "KEYS a b c");

    assert_eq!(send_cmd(&mut writer, &mut reader, "SET a 999"), "OK");
    assert_eq!(send_cmd(&mut writer, &mut reader, "GET a"), "VALUE a 999");

    assert_eq!(send_cmd(&mut writer, &mut reader, "DEL b"), "OK");
    assert_eq!(
        send_cmd(&mut writer, &mut reader, "STATUS"),
        "STATUS count=2"
    );
}

#[test]
fn net_overwrite() {
    let addr = start_test_server();
    let (mut reader, mut writer) = connect(&addr);

    assert_eq!(send_cmd(&mut writer, &mut reader, "SET k v1"), "OK");
    assert_eq!(send_cmd(&mut writer, &mut reader, "SET k v2"), "OK");
    assert_eq!(send_cmd(&mut writer, &mut reader, "GET k"), "VALUE k v2");
}

/// 批量写入 50 条，验证数量和列表
#[test]
fn net_batch_write_and_list() {
    let addr = start_test_server();
    let (mut reader, mut writer) = connect(&addr);

    for i in 0..50 {
        assert_eq!(
            send_cmd(
                &mut writer,
                &mut reader,
                &format!("SET key{} value{}", i, i)
            ),
            "OK"
        );
    }

    assert_eq!(
        send_cmd(&mut writer, &mut reader, "STATUS"),
        "STATUS count=50"
    );

    let list_resp = send_cmd(&mut writer, &mut reader, "LIST");
    assert!(list_resp.starts_with("KEYS "));
    let keys: Vec<&str> = list_resp.trim_start_matches("KEYS ").split(' ').collect();
    assert_eq!(keys.len(), 50);
}

// ============================================================
// 四、错误场景：错误不中断连接
// ============================================================

#[test]
fn net_unknown_command_does_not_break() {
    let addr = start_test_server();
    let (mut reader, mut writer) = connect(&addr);

    let resp = send_cmd(&mut writer, &mut reader, "FOOBAR");
    assert!(
        resp.starts_with("ERROR"),
        "未知命令应返回 ERROR，实际: {}",
        resp
    );

    assert_eq!(send_cmd(&mut writer, &mut reader, "PING"), "PONG");
    assert_eq!(send_cmd(&mut writer, &mut reader, "SET x 1"), "OK");
    assert_eq!(send_cmd(&mut writer, &mut reader, "GET x"), "VALUE x 1");
}

#[test]
fn net_missing_arg_does_not_break() {
    let addr = start_test_server();
    let (mut reader, mut writer) = connect(&addr);

    let resp = send_cmd(&mut writer, &mut reader, "SET onlykey");
    assert!(resp.starts_with("ERROR"), "缺参数应返回 ERROR");

    assert_eq!(send_cmd(&mut writer, &mut reader, "PING"), "PONG");
}

#[test]
fn net_extra_arg_does_not_break() {
    let addr = start_test_server();
    let (mut reader, mut writer) = connect(&addr);

    let resp = send_cmd(&mut writer, &mut reader, "PING extra_arg");
    assert!(
        resp.starts_with("ERROR"),
        "PING带多余参数应返回 ERROR，实际: {}",
        resp
    );

    assert_eq!(send_cmd(&mut writer, &mut reader, "PING"), "PONG");
    assert_eq!(
        send_cmd(&mut writer, &mut reader, "STATUS"),
        "STATUS count=0"
    );
}

/// 连续多次出错，连接仍然可用
#[test]
fn net_multiple_errors_still_works() {
    let addr = start_test_server();
    let (mut reader, mut writer) = connect(&addr);

    // 连错三次
    assert!(send_cmd(&mut writer, &mut reader, "BADCMD1").starts_with("ERROR"));
    assert!(send_cmd(&mut writer, &mut reader, "BADCMD2").starts_with("ERROR"));
    assert!(send_cmd(&mut writer, &mut reader, "GET").starts_with("ERROR"));

    // 连接还在
    assert_eq!(send_cmd(&mut writer, &mut reader, "SET ok yes"), "OK");
    assert_eq!(send_cmd(&mut writer, &mut reader, "GET ok"), "VALUE ok yes");
    assert_eq!(
        send_cmd(&mut writer, &mut reader, "STATUS"),
        "STATUS count=1"
    );
}

// ============================================================
// 五、EXIT 与断开
// ============================================================

#[test]
fn net_exit_closes_connection() {
    let addr = start_test_server();
    let (mut reader, mut writer) = connect(&addr);

    assert_eq!(send_cmd(&mut writer, &mut reader, "PING"), "PONG");

    writer.write_all(b"EXIT\n").unwrap();
    writer.flush().unwrap();

    let mut buf = String::new();
    let n = reader.read_line(&mut buf).unwrap_or(0);
    assert_eq!(n, 0, "EXIT 后服务器应关闭连接");
}

#[test]
fn net_client_disconnect_graceful() {
    let addr = start_test_server();
    let (_, mut writer) = connect(&addr);

    writer.write_all(b"SET a 1\n").unwrap();
    writer.flush().unwrap();
    drop(writer);

    thread::sleep(Duration::from_millis(50));
    // 没有 panic 就算通过
}

// ============================================================
// 六、值的特殊场景
// ============================================================

#[test]
fn net_value_with_spaces() {
    let addr = start_test_server();
    let (mut reader, mut writer) = connect(&addr);

    assert_eq!(
        send_cmd(&mut writer, &mut reader, "SET msg hello world"),
        "OK"
    );
    assert_eq!(
        send_cmd(&mut writer, &mut reader, "GET msg"),
        "VALUE msg hello world"
    );
}

#[test]
fn net_value_with_chinese() {
    let addr = start_test_server();
    let (mut reader, mut writer) = connect(&addr);

    assert_eq!(
        send_cmd(&mut writer, &mut reader, "SET greeting 你好世界"),
        "OK"
    );
    assert_eq!(
        send_cmd(&mut writer, &mut reader, "GET greeting"),
        "VALUE greeting 你好世界"
    );
}

#[test]
fn net_case_insensitive_commands() {
    let addr = start_test_server();
    let (mut reader, mut writer) = connect(&addr);

    assert_eq!(send_cmd(&mut writer, &mut reader, "set k v"), "OK");
    assert_eq!(send_cmd(&mut writer, &mut reader, "Get k"), "VALUE k v");
    assert_eq!(send_cmd(&mut writer, &mut reader, "ping"), "PONG");
    assert_eq!(send_cmd(&mut writer, &mut reader, "LIST"), "KEYS k");
}

// ============================================================
// 七、空行忽略
// ============================================================

#[test]
fn net_empty_line_ignored() {
    let addr = start_test_server();
    let (mut reader, mut writer) = connect(&addr);

    writer.write_all(b"\n").unwrap();
    writer.flush().unwrap();

    assert_eq!(send_cmd(&mut writer, &mut reader, "PING"), "PONG");
}
