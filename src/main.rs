//! kvstore —— 可持久化网络键值存储系统
//!
//! 二进制入口：模块定义统一放在 src/lib.rs（库 crate），
//! 本文件只负责按命令行参数分发启动模式。

use kvstore::client;
use kvstore::protocol::DEFAULT_ADDR;
use kvstore::server::{self, Server};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("server") => server::run(&args[2..]),
        Some("client") => {
            if args.iter().any(|a| a == "--local") {
                run_local_client();
            } else {
                client::run_tcp_repl(DEFAULT_ADDR);
            }
        }
        Some("--version") | Some("-V") => print_version(),
        _ => print_help(),
    }
}

/// 本地客户端（第2天模式：不经网络，复用 server 的执行逻辑；EXIT 由 REPL 处理）
///
/// 数据写临时文件，避免污染服务器数据文件；第3天起默认使用 TCP 客户端。
fn run_local_client() {
    // 临时数据文件，防止本地调试污染 data/kv.log
    let path = std::env::temp_dir().join("kvstore_local_client.log");
    let mut server = Server::new(path.to_str().unwrap());
    client::run_local_repl(|parsed| match server.execute(&parsed) {
        Some(reply) => reply,
        None => "BYE".to_string(),
    });
}

/// 打印版本与模块清单
fn print_version() {
    println!("kvstore v{}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("模块清单:");
    println!("  protocol    - 命令与消息格式定义【已定稿】");
    println!("  parser      - 命令解析与合法性校验（成员C）【已实现】");
    println!("  store       - 内存键值存储（成员B）【已实现】");
    println!("  persistence - 追加日志与启动恢复（成员B）");
    println!("  server      - TCP监听与连接处理（成员A）");
    println!("  client      - 命令行客户端（成员C）");
    println!("  tests       - 集成测试（成员D）");
}

/// 打印帮助信息
fn print_help() {
    print_version();
    println!();
    println!("用法:");
    println!("  cargo run -- server          启动服务器（第3天：网络模式，监听 {}）", DEFAULT_ADDR);
    println!("  cargo run -- server --local  启动服务器（本地模式，无网络）");
    println!("  cargo run -- client          启动客户端（第3天：连接服务器）");
    println!("  cargo run -- client --local  启动客户端（本地模式，无网络）");
    println!("  cargo run -- --version       显示版本与模块清单");
    println!("  cargo run -- --help          显示本帮助");
    println!();
    println!("开发计划:");
    println!("  第3天: TCP网络通信 + 持久化");
    println!("  第4天: 多客户端并发 + 测试 + 演示");
}
