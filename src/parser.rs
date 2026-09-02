//! 命令解析模块（成员C负责）
//!
//! 第1天：定义解析结果类型与函数签名（骨架，可编译）。
//! 第2天：成员C实现完整解析与合法性校验：
//!   - 全部命令 SET/GET/DEL/LIST/STATUS/PING/EXIT；
//!   - 未知命令、缺少参数、多余参数、非法键的明确中文错误提示；
//!   - 单条命令出错不影响后续命令。

use crate::protocol::{Command, MAX_MSG_LEN, error};
use std::fmt;
const MAX_TTL_SECONDS: u64 = 86_400 * 365;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ERROR {}", self.message)
    }
}

// 这是你解析后交给服务器的完整命令。
// Command 只表示“命令类型”；key 和 value 保存用户实际输入的参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCommand {
    pub command: Command,
    pub key: Option<String>,
    pub value: Option<String>,
    pub ttl: Option<u64>,
}

// 从一段文本中取出第一个单词，返回：
// ("SET", " name Alice")
// ("name", " Alice")
fn take_word(input: &str) -> Option<(&str, &str)> {
    let input = input.trim_start();

    if input.is_empty() {
        return None;
    }

    let end = input.find(char::is_whitespace).unwrap_or(input.len());

    Some((&input[..end], &input[end..]))
}

// 检查 key 是否合法。
fn validate_key(key: &str) -> Result<(), ParseError> {
    if key.is_empty() {
        return Err(ParseError {
            message: format!("{}：key 不能为空", error::INVALID_KEY),
        });
    }

    if key.contains(char::is_whitespace) || key.contains('\n') || key.contains('\r') {
        return Err(ParseError {
            message: format!("{}：key 不能包含空格或换行", error::INVALID_KEY),
        });
    }

    Ok(())
}

// GET / DEL：只能有一个 key 参数。
fn parse_one_key(rest: &str, command_name: &str) -> Result<String, ParseError> {
    let (key, remain) = take_word(rest).ok_or_else(|| ParseError {
        message: format!("{}：{} 需要 key", error::MISSING_ARG, command_name),
    })?;

    if !remain.trim().is_empty() {
        return Err(ParseError {
            message: format!("{}：{} 只能有一个 key", error::EXTRA_ARG, command_name),
        });
    }

    validate_key(key)?;

    Ok(key.to_string())
}

// LIST / STATUS / PING / EXIT：不接受参数。
fn check_no_argument(rest: &str, command_name: &str) -> Result<(), ParseError> {
    if !rest.trim().is_empty() {
        return Err(ParseError {
            message: format!("{}：{} 不需要参数", error::EXTRA_ARG, command_name),
        });
    }

    Ok(())
}
fn parse_value_and_ttl(value_part: &str) -> Result<(String, Option<u64>), ParseError> {
    let value_part = value_part.trim();

    if value_part.is_empty() {
        return Err(ParseError {
            message: format!("{}：SET 需要 value", error::MISSING_ARG),
        });
    }

    // 只有最后一个字段像 TTL 时，才把它当作 TTL。
    // 这样可以保留 value 中的普通空格。
    let split_position = value_part.rfind(char::is_whitespace);

    let Some(position) = split_position else {
        return Ok((value_part.to_string(), None));
    };

    let value = value_part[..position].trim_end();
    let candidate = value_part[position..].trim();

    // 例如：SET key hello world
    // world 不是数字，因此整体作为 value。
    let looks_like_ttl =
        candidate.starts_with('-') || candidate.chars().all(|c| c.is_ascii_digit());

    if !looks_like_ttl {
        return Ok((value_part.to_string(), None));
    }

    if value.is_empty() {
        return Err(ParseError {
            message: format!("{}：SET 需要 value", error::MISSING_ARG),
        });
    }

    let ttl = candidate.parse::<u64>().map_err(|_| ParseError {
        message: format!("{}：TTL 必须是正整数", error::INVALID_KEY),
    })?;

    if ttl == 0 {
        return Err(ParseError {
            message: "TTL 必须大于 0".to_string(),
        });
    }

    if ttl > MAX_TTL_SECONDS {
        return Err(ParseError {
            message: format!("TTL 不能超过 {} 秒", MAX_TTL_SECONDS),
        });
    }

    Ok((value.to_string(), Some(ttl)))
}
// 这是你的主要函数：把一行用户输入转换为 ParsedCommand。
pub fn parse_command(line: &str) -> Result<ParsedCommand, ParseError> {
    if line.as_bytes().len() > MAX_MSG_LEN {
        return Err(ParseError {
            message: format!(
                "{}：单条命令不能超过 {} 字节",
                error::MSG_TOO_LONG,
                MAX_MSG_LEN
            ),
        });
    }

    // 去掉用户按回车输入产生的换行符。
    let line = line.trim_end_matches('\n').trim_end_matches('\r');
    let line = line.trim();

    if line.is_empty() {
        return Err(ParseError {
            message: format!("{}：命令不能为空", error::UNKNOWN_COMMAND),
        });
    }

    let (command_text, rest) = take_word(line).unwrap();

    // 使用组长已经写好的 from_str。
    // 它规定命令大小写不敏感，所以 set / SET / Set 都可以。
    let command = Command::from_str(command_text).ok_or_else(|| ParseError {
        message: format!("{}：{}", error::UNKNOWN_COMMAND, command_text),
    })?;

    match command {
        Command::Set => {
            let (key, value_part) = take_word(rest).ok_or_else(|| ParseError {
                message: format!("{}：SET 需要 key 和 value", error::MISSING_ARG),
            })?;

            validate_key(key)?;

            let (value, ttl) = parse_value_and_ttl(value_part)?;

            Ok(ParsedCommand {
                command: Command::Set,
                key: Some(key.to_string()),
                value: Some(value),
                ttl,
            })
        }

        Command::Get => Ok(ParsedCommand {
            command: Command::Get,
            key: Some(parse_one_key(rest, "GET")?),
            value: None,
            ttl: None,
        }),

        Command::Del => Ok(ParsedCommand {
            command: Command::Del,
            key: Some(parse_one_key(rest, "DEL")?),
            value: None,
            ttl: None,
        }),

        Command::List => {
            check_no_argument(rest, "LIST")?;

            Ok(ParsedCommand {
                command: Command::List,
                key: None,
                value: None,
                ttl: None,
            })
        }

        Command::Status => {
            check_no_argument(rest, "STATUS")?;

            Ok(ParsedCommand {
                command: Command::Status,
                key: None,
                value: None,
                ttl: None,
            })
        }

        Command::Ping => {
            check_no_argument(rest, "PING")?;

            Ok(ParsedCommand {
                command: Command::Ping,
                key: None,
                value: None,
                ttl: None,
            })
        }

        Command::Exit => {
            check_no_argument(rest, "EXIT")?;

            Ok(ParsedCommand {
                command: Command::Exit,
                key: None,
                value: None,
                ttl: None,
            })
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Command;

    #[test]
    fn can_parse_set() {
        let result = parse_command("SET name Alice").unwrap();

        assert_eq!(result.command, Command::Set);
        assert_eq!(result.key, Some("name".to_string()));
        assert_eq!(result.value, Some("Alice".to_string()));
    }

    #[test]
    fn set_value_can_contain_spaces() {
        let result = parse_command("SET sentence Rust is easy").unwrap();

        assert_eq!(result.command, Command::Set);
        assert_eq!(result.key, Some("sentence".to_string()));
        assert_eq!(result.value, Some("Rust is easy".to_string()));
    }

    #[test]
    fn can_parse_get() {
        let result = parse_command("GET name").unwrap();

        assert_eq!(result.command, Command::Get);
        assert_eq!(result.key, Some("name".to_string()));
        assert_eq!(result.value, None);
    }

    #[test]
    fn can_parse_command_in_lowercase() {
        let result = parse_command("get name").unwrap();

        assert_eq!(result.command, Command::Get);
    }

    #[test]
    fn missing_key_returns_error() {
        let error = parse_command("GET").unwrap_err();

        assert!(error.message.contains("缺少参数"));
    }

    #[test]
    fn set_without_value_returns_error() {
        let error = parse_command("SET name").unwrap_err();

        assert!(error.message.contains("缺少参数"));
    }

    #[test]
    fn extra_argument_returns_error() {
        let error = parse_command("GET name extra").unwrap_err();

        assert!(error.message.contains("多余参数"));
    }

    #[test]
    fn unknown_command_returns_error() {
        let error = parse_command("HELLO").unwrap_err();

        assert!(error.message.contains("未知命令"));
    }
    #[test]
    fn set_with_ttl() {
        let result = parse_command("SET course Rust 5").unwrap();

        assert_eq!(result.command, Command::Set);
        assert_eq!(result.key, Some("course".to_string()));
        assert_eq!(result.value, Some("Rust".to_string()));
        assert_eq!(result.ttl, Some(5));
    }

    #[test]
    fn set_without_ttl_is_permanent() {
        let result = parse_command("SET course Rust").unwrap();

        assert_eq!(result.value, Some("Rust".to_string()));
        assert_eq!(result.ttl, None);
    }

    #[test]
    fn set_value_can_contain_spaces_without_ttl() {
        let result = parse_command("SET sentence Rust is easy").unwrap();

        assert_eq!(result.value, Some("Rust is easy".to_string()));
        assert_eq!(result.ttl, None);
    }

    #[test]
    fn ttl_zero_is_rejected() {
        let error = parse_command("SET key value 0").unwrap_err();

        assert!(error.message.contains("TTL"));
    }

    #[test]
    fn ttl_negative_is_rejected() {
        let error = parse_command("SET key value -1").unwrap_err();

        assert!(error.message.contains("TTL"));
    }

    #[test]
    fn ttl_too_large_is_rejected() {
        let error = parse_command("SET key value 999999999999").unwrap_err();

        assert!(error.message.contains("TTL"));
    }

    #[test]
    fn non_set_commands_have_no_ttl() {
        let result = parse_command("GET key").unwrap();

        assert_eq!(result.ttl, None);
    }
}
