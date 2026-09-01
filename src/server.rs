//! 服务器模块（成员A负责）
//!
//! 第1天：定义入口函数签名（骨架，可编译）。
//! 第2天：实现主循环：stdin读命令 → parser解析 → store执行 → 打印结果；
//!        保证单条命令出错后程序继续运行。
//! 第3天：接入 TcpListener 监听 DEFAULT_ADDR，每连接一线程；
//!        用 BufReader 逐行读取请求（处理消息分段）。
//! 第4天：启动参数化（端口、数据文件路径）；并发安全共享；错误隔离。

use crate::protocol::DEFAULT_ADDR;

/// 启动服务器（第2天起由 main 调用）
pub fn run() {
    // TODO(成员A): 第2天实现stdin主循环；第3天实现TCP监听
    println!(
        "服务器模块骨架（第2天实现主循环，监听地址: {}）",
        DEFAULT_ADDR
    );
}
