//! 协议契约（第1天定稿，全组必须遵守）
//!
//! 消息格式约定（详见项目根目录 PROTOCOL.md）：
//!   - 一行一条请求/响应，字段以空格分隔，以换行符 \n 结尾；
//!   - 键不允许包含空格或换行，值不允许换行（值中的空格允许）；
//!   - 单条命令出错不影响连接，后续命令继续正常处理。
//!
//! 依赖关系：成员C的 parser、成员A的 server、成员D的测试都以此模块为准。

/// 支持的命令（第1天定稿；成员C据此实现解析器）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// SET key value —— 写入或覆盖键值
    Set,
    /// GET key —— 查询键值
    Get,
    /// DEL key —— 删除键值
    Del,
    /// LIST —— 列出全部键
    List,
    /// STATUS —— 查看数据数量与运行状态
    Status,
    /// PING —— 检查连接
    Ping,
    /// EXIT —— 客户端退出并关闭连接
    Exit,
}

impl Command {
    /// 从命令名字符串解析（大小写不敏感；成员C将在 parser 中做完整校验）
    pub fn from_str(name: &str) -> Option<Command> {
        match name.to_uppercase().as_str() {
            "SET" => Some(Command::Set),
            "GET" => Some(Command::Get),
            "DEL" => Some(Command::Del),
            "LIST" => Some(Command::List),
            "STATUS" => Some(Command::Status),
            "PING" => Some(Command::Ping),
            "EXIT" => Some(Command::Exit),
            _ => None,
        }
    }

    /// 命令的规范名称（大写）
    pub fn name(&self) -> &'static str {
        match self {
            Command::Set => "SET",
            Command::Get => "GET",
            Command::Del => "DEL",
            Command::List => "LIST",
            Command::Status => "STATUS",
            Command::Ping => "PING",
            Command::Exit => "EXIT",
        }
    }

    /// 该命令需要的参数个数（用于校验缺参数/多参数）
    pub fn required_args(&self) -> usize {
        match self {
            Command::Set => 2,                // key value
            Command::Get | Command::Del => 1, // key
            Command::List | Command::Status | Command::Ping | Command::Exit => 0,
        }
    }
}

/// 错误码（响应统一格式：`ERROR <说明>`，说明使用中文，清晰可读）
pub mod error {
    /// 未知命令
    pub const UNKNOWN_COMMAND: &str = "未知命令";
    /// 缺少参数
    pub const MISSING_ARG: &str = "缺少参数";
    /// 多余参数
    pub const EXTRA_ARG: &str = "多余参数";
    /// 键不存在
    pub const KEY_NOT_FOUND: &str = "键不存在";
    /// 键不合法（空键、含空格等）
    pub const INVALID_KEY: &str = "非法键";
    /// 消息超长
    pub const MSG_TOO_LONG: &str = "消息超长";
}

/// 默认监听地址与端口（第3天起使用，成员A负责）
pub const DEFAULT_ADDR: &str = "127.0.0.1:7878";

/// 默认端口（第4天参数化：--port 可覆盖）
pub const DEFAULT_PORT: u16 = 7878;

/// 单条消息最大长度（字节），超过视为非法并返回 MSG_TOO_LONG（第3天实现校验）
pub const MAX_MSG_LEN: usize = 1024;

/// 默认数据文件路径（第3天起使用，成员B负责）
pub const DEFAULT_DATA_FILE: &str = "data/kv.log";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_from_str_is_case_insensitive() {
        assert_eq!(Command::from_str("set"), Some(Command::Set));
        assert_eq!(Command::from_str("GET"), Some(Command::Get));
        assert_eq!(Command::from_str("List"), Some(Command::List));
        assert_eq!(Command::from_str("FOO"), None);
    }

    #[test]
    fn command_required_args() {
        assert_eq!(Command::Set.required_args(), 2);
        assert_eq!(Command::Get.required_args(), 1);
        assert_eq!(Command::Ping.required_args(), 0);
    }
}
