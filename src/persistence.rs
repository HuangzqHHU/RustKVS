//! 持久化模块（成员B负责，成员D配合定义格式）
//!
//! 第1天：定义日志记录格式与接口（骨架，可编译）。
//! 第3天：实现追加式日志、启动重放恢复、文件异常检测。
//!
//! 约定（与成员D商定）：
//!   - 日志文件 data/kv.log，每行一条记录，字段以空格分隔；
//!   - `SET <key> <value>` 表示写入或覆盖；`DEL <key>` 表示删除；
//!   - 先写日志文件、成功后更新内存，再向客户端返回成功（已确认数据不丢失）；
//!   - 文件截断/损坏/格式异常时返回明确错误并退出，绝不静默清空。

use std::io::{BufRead, Write};

use crate::store::KVStore;

/// 日志记录格式（第1天与成员D商定，第3天实现读写）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogRecord {
    /// 写入或覆盖
    Set { key: String, value: String },
    /// 删除
    Del { key: String },
}

impl LogRecord {
    /// 序列化为日志文件中的一行文本
    pub fn to_line(&self) -> String {
        match self {
            LogRecord::Set { key, value } => format!("SET {} {}", key, value),
            LogRecord::Del { key } => format!("DEL {}", key),
        }
    }

    /// 从一行文本解析（无法识别时返回 None）
    ///
    /// 与协议一致：`SET` 取第二个空格之后的整体为值，因此值允许包含空格。
    /// 命令名大小写不敏感；空键、缺参数、多余参数均视为无法识别。
    pub fn from_line(line: &str) -> Option<LogRecord> {
        // 去掉行尾换行符（不影响值末尾的空格）
        let line = line.trim_end_matches(|c: char| c == '\n' || c == '\r');
        let (cmd, rest) = line.split_once(' ')?;

        if cmd.eq_ignore_ascii_case("SET") {
            let (key, value) = rest.split_once(' ')?;
            if key.is_empty() {
                return None;
            }
            Some(LogRecord::Set {
                key: key.to_string(),
                value: value.to_string(),
            })
        } else if cmd.eq_ignore_ascii_case("DEL") {
            if rest.is_empty() || rest.contains(' ') {
                return None;
            }
            Some(LogRecord::Del {
                key: rest.to_string(),
            })
        } else {
            None
        }
    }
}

/// 持久化接口（第3天由成员B实现）
pub struct Persistence {
    /// 日志文件路径（默认 data/kv.log）
    log_path: std::path::PathBuf,
}

impl Persistence {
    /// 新建持久化实例
    pub fn new(log_path: impl Into<std::path::PathBuf>) -> Self {
        Persistence {
            log_path: log_path.into(),
        }
    }

    /// 将一条修改记录追加到日志文件
    ///
    /// 以追加模式打开文件（不存在则创建），写入一行并 flush 落盘。
    pub fn append(&self, record: &LogRecord) -> std::io::Result<()> {
        // 数据目录在运行时生成，不存在则先创建
        if let Some(parent) = self.log_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)?;
        writeln!(file, "{}", record.to_line())?;
        file.flush()
    }

    /// 启动时重放日志，恢复到上次运行结束时的状态
    ///
    /// - 文件不存在视为首次启动，返回 Ok（空库）；
    /// - 逐行解析并重放，写回 `store`；
    /// - 文件截断/损坏/格式异常返回 `Err(明确说明)`，绝不静默清空。
    pub fn recover(&self, store: &mut KVStore) -> Result<(), String> {
        let file = match std::fs::File::open(&self.log_path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(format!("打开日志文件失败: {}", e)),
        };

        let reader = std::io::BufReader::new(file);
        for (idx, line) in reader.lines().enumerate() {
            let line = line.map_err(|e| format!("读取日志文件第 {} 行失败: {}", idx + 1, e))?;
            if line.trim().is_empty() {
                return Err(format!("日志文件第 {} 行为空（文件可能损坏）", idx + 1));
            }
            match LogRecord::from_line(&line) {
                Some(LogRecord::Set { key, value }) => {
                    store
                        .set(&key, &value)
                        .map_err(|e| format!("日志文件第 {} 行非法: {}", idx + 1, e))?;
                }
                Some(LogRecord::Del { key }) => {
                    store
                        .delete(&key)
                        .map_err(|e| format!("日志文件第 {} 行非法: {}", idx + 1, e))?;
                }
                None => {
                    return Err(format!(
                        "日志文件第 {} 行格式错误: {:?}",
                        idx + 1,
                        line.trim_end()
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// 生成隔离的临时日志路径，避免并发测试互相干扰
    fn temp_log(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "kvstore_persist_{}_{}.log",
            std::process::id(),
            name
        ))
    }

    #[test]
    fn to_line_formats_set_and_del() {
        assert_eq!(
            LogRecord::Set {
                key: "k".into(),
                value: "v".into()
            }
            .to_line(),
            "SET k v"
        );
        assert_eq!(LogRecord::Del { key: "k".into() }.to_line(), "DEL k");
    }

    #[test]
    fn from_line_parses_set_with_spaces_in_value() {
        assert_eq!(
            LogRecord::from_line("SET k hello world !"),
            Some(LogRecord::Set {
                key: "k".into(),
                value: "hello world !".into()
            })
        );
    }

    #[test]
    fn from_line_parses_del() {
        assert_eq!(
            LogRecord::from_line("DEL k"),
            Some(LogRecord::Del { key: "k".into() })
        );
    }

    #[test]
    fn from_line_is_case_insensitive() {
        assert_eq!(
            LogRecord::from_line("set k v"),
            Some(LogRecord::Set {
                key: "k".into(),
                value: "v".into()
            })
        );
        assert_eq!(
            LogRecord::from_line("del k"),
            Some(LogRecord::Del { key: "k".into() })
        );
    }

    #[test]
    fn from_line_rejects_bad_lines() {
        assert_eq!(LogRecord::from_line("FOO k"), None);
        assert_eq!(LogRecord::from_line("SET"), None); // 缺 key/value
        assert_eq!(LogRecord::from_line("SET k"), None); // 缺 value
        assert_eq!(LogRecord::from_line("SET  v"), None); // 空键
        assert_eq!(LogRecord::from_line("DEL"), None); // 缺 key
        assert_eq!(LogRecord::from_line("DEL a b"), None); // 多余参数
        assert_eq!(LogRecord::from_line(""), None);
    }

    #[test]
    fn to_line_and_from_line_roundtrip() {
        let recs = vec![
            LogRecord::Set {
                key: "a".into(),
                value: "1".into(),
            },
            LogRecord::Set {
                key: "b".into(),
                value: "with space".into(),
            },
            LogRecord::Del { key: "a".into() },
        ];
        for rec in recs {
            let line = rec.to_line();
            assert_eq!(LogRecord::from_line(&line), Some(rec));
        }
    }

    #[test]
    fn append_and_recover_roundtrip() {
        let path = temp_log("roundtrip");
        let _ = std::fs::remove_file(&path);
        let p = Persistence::new(&path);

        p.append(&LogRecord::Set {
            key: "name".into(),
            value: "kvstore".into(),
        })
        .unwrap();
        p.append(&LogRecord::Set {
            key: "msg".into(),
            value: "hello world".into(),
        })
        .unwrap();
        p.append(&LogRecord::Del { key: "msg".into() }).unwrap();

        let mut store = KVStore::new();
        p.recover(&mut store).unwrap();
        assert_eq!(store.get("name"), Ok(Some("kvstore")));
        assert_eq!(store.get("msg"), Ok(None)); // 已按日志删除
        assert_eq!(store.len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn recover_missing_file_is_ok_and_empty() {
        let path = temp_log("missing");
        let _ = std::fs::remove_file(&path);
        let p = Persistence::new(&path);

        let mut store = KVStore::new();
        p.recover(&mut store).unwrap();
        assert!(store.is_empty());
    }

    #[test]
    fn recover_errors_on_corrupt_line() {
        let path = temp_log("corrupt");
        std::fs::write(&path, "SET k v\nGARBAGE\n").unwrap();
        let p = Persistence::new(&path);

        let mut store = KVStore::new();
        assert!(p.recover(&mut store).is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn recover_errors_on_truncated_line() {
        let path = temp_log("truncated");
        std::fs::write(&path, "SET k\n").unwrap(); // 缺 value
        let p = Persistence::new(&path);

        let mut store = KVStore::new();
        assert!(p.recover(&mut store).is_err());
        let _ = std::fs::remove_file(&path);
    }
}
