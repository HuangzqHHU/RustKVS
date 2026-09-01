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
    /// 序列化为日志文件中的一行文本（第3天实现）
    pub fn to_line(&self) -> String {
        // TODO(成员B): 第3天实现，格式：`SET <key> <value>` / `DEL <key>`
        match self {
            LogRecord::Set { key, value } => format!("SET {} {}", key, value),
            LogRecord::Del { key } => format!("DEL {}", key),
        }
    }

    /// 从一行文本解析（第3天实现；无法识别时返回 None）
    pub fn from_line(line: &str) -> Option<LogRecord> {
        // TODO(成员B): 第3天实现
        let _ = line;
        None
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
        Persistence { log_path: log_path.into() }
    }

    /// 将一条修改记录追加到日志文件（第3天实现）
    pub fn append(&self, record: &LogRecord) -> std::io::Result<()> {
        // TODO(成员B): 第3天实现：打开文件以追加模式写入一行并 flush
        let _ = record;
        Ok(())
    }

    /// 启动时重放日志，恢复到上次运行结束时的状态（第3天实现）
    ///
    /// 返回 Err 表示文件异常，调用方（服务器）应给出明确错误并退出。
    pub fn recover(&self, store: &mut KVStore) -> Result<(), String> {
        // TODO(成员B): 第3天实现；逐行 LogRecord::from_line 重放；异常时 Err(明确说明)
        let _ = store;
        Ok(())
    }
}
