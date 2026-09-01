//! kvstore —— 可持久化网络键值存储系统
//!
//! 入口：按命令行参数启动不同模式。
//! 第1天：仅支持 --version / --help。
//! 第2天：支持 `server`（本地stdin主循环，无网络）。
//! 第3天：支持 `client`（TCP连接）。

mod client;
mod parser;
mod persistence;
mod protocol;
mod server;
mod store;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        Some("server") => server::run(),
        Some("client") => client::run(),
        Some("--version") | Some("-V") => print_version(),
        _ => print_help(),
    }
}

/// 打印版本与模块清单
fn print_version() {
    println!("kvstore v{}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("模块清单:");
    println!("  protocol    - 命令与消息格式定义【已定稿】");
    println!("  parser      - 命令解析与合法性校验（成员C）");
    println!("  store       - 内存键值存储（成员B）");
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
    println!("  cargo run -- server         启动服务器（第2天：本地模式，无网络）");
    println!("  cargo run -- client         启动客户端（第3天起可用）");
    println!("  cargo run -- --version      显示版本与模块清单");
    println!("  cargo run -- --help         显示本帮助");
    println!();
    println!("开发计划:");
    println!("  第2天: 本地主循环跑通增删改查（依赖成员B/C实现合并）");
    println!("  第3天: TCP网络通信 + 持久化");
    println!("  第4天: 多客户端并发 + 测试 + 演示");
}
