//! 命令解析模块（成员C负责）
//!
//! 第1天：定义解析结果类型与函数签名（骨架，可编译）。
//! 第2天：成员C实现完整解析与合法性校验：
//!   - 全部命令 SET/GET/DEL/LIST/STATUS/PING/EXIT；
//!   - 未知命令、缺少参数、多余参数、非法键的明确中文错误提示；
//!   - 单条命令出错不影响后续命令。

use crate::protocol::Command;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ERROR {}", self.message)
    }
}

pub fn parse_command(_line: &str) -> Result<Command, ParseError> {
    Err(ParseError {
        message: "解析器暂未实现".to_string(),
    })
}