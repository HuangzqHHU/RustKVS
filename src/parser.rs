//! 命令解析模块（成员C负责）
//!
//! 第1天：定义解析结果类型与函数签名（骨架，可编译）。
//! 第2天：成员C实现完整解析与合法性校验：
//!   - 全部命令 SET/GET/DEL/LIST/STATUS/PING/EXIT；
//!   - 未知命令、缺少参数、多余参数、非法键的明确中文错误提示；
//!   - 单条命令出错不影响后续命令。

use crate::protocol::Command;

/// 解析成功后的命令表示
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCommand {
    /// 命令类型
    pub cmd: Command,
    /// 参数列表（如 SET 的 [key, value]、GET 的 [key]；无参数命令为空）
    pub args: Vec<String>,
}

/// 解析错误（message 为可直接展示给用户的中文提示）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
}

/// 将一行用户输入解析为命令（第2天由成员C实现完整校验）
pub fn parse(line: &str) -> Result<ParsedCommand, ParseError> {
    // TODO(成员C): 第2天实现。
    // 提示：
    //   1) 用 split_whitespace 切分，取首词作为命令名（大小写不敏感）；
    //   2) 用 Command::from_str 识别命令，返回 None 时给 UNKNOWN_COMMAND；
    //   3) 用 Command::required_args() 校验参数个数；
    //   4) 校验键合法性（非空、不含空格、不含换行）。
    let _ = line;
    Err(ParseError { message: "解析器待实现（第2天，成员C）".to_string() })
}
