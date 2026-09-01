//! kvstore 库入口
//!
//! 将各模块暴露为库 crate，使 tests/ 下的集成测试可以导入。
//! 二进制入口仍在 main.rs，通过 `use kvstore::*` 调用。

pub mod client;
pub mod parser;
pub mod persistence;
pub mod protocol;
pub mod server;
pub mod store;
