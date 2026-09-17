use rusqlite::{params, Connection};
use serde::Serialize;
use std::{fs, path::Path, sync::Mutex};
use tauri::{AppHandle, Manager};

const MAX_QUESTION_BYTES: usize = 10_000;
const MAX_ANSWER_BYTES: usize = 1_000_000;
const MAX_RECENT_TURNS: i64 = 50;
const MAX_MEMORIES: i64 = 100;
const MAX_MEMORY_TITLE_BYTES: usize = 120;
const MAX_MEMORY_CONTENT_BYTES: usize = 600;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HistoryTurn {
    #[serde(rename = "runId")]
    pub run_id: String,
    pub question: String,
    pub answer: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ExplicitMemory {
    pub id: String,
    pub title: String,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Default)]
pub struct HistoryStore(Mutex<Option<Connection>>);

fn initialize(connection: &mut Connection) -> Result<(), String> {
    connection
        .execute_batch("PRAGMA foreign_keys=ON; PRAGMA secure_delete=ON; PRAGMA journal_mode=DELETE; PRAGMA synchronous=EXTRA;")
        .map_err(|_| "无法初始化历史数据库")?;
    let scrub: i64 = connection
        .query_row("PRAGMA secure_delete", [], |row| row.get(0))
        .map_err(|_| "无法验证历史清理策略")?;
    let journal: String = connection
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .map_err(|_| "无法验证历史事务模式")?;
    if scrub != 1 || !matches!(journal.as_str(), "delete" | "memory") {
        return Err("历史数据库的清理策略不安全；不会写入聊天内容".into());
    }
    let version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|_| "无法读取历史数据库版本")?;
    if version > 2 {
        return Err("历史数据库版本比当前应用更新；不会覆盖它".into());
    }
    if version == 0 {
        let transaction = connection.transaction().map_err(|_| "无法迁移历史数据库")?;
        transaction
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                conversation_id TEXT NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
                turn_id TEXT NOT NULL,
                role TEXT NOT NULL CHECK(role IN ('user', 'assistant')),
                content TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(turn_id, role)
            );
            CREATE INDEX IF NOT EXISTS messages_recent ON messages(role, id DESC);
            INSERT OR IGNORE INTO schema_migrations(version) VALUES (1);
            PRAGMA user_version=1;",
            )
            .map_err(|_| "无法迁移历史数据库")?;
        transaction.commit().map_err(|_| "无法提交历史数据库迁移")?;
    }
    if version <= 1 {
        let transaction = connection.transaction().map_err(|_| "无法迁移记忆数据库")?;
        transaction
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                source TEXT NOT NULL DEFAULT 'user_explicit' CHECK(source='user_explicit'),
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS memories_updated ON memories(updated_at DESC, id);
            INSERT OR IGNORE INTO schema_migrations(version) VALUES (2);
            PRAGMA user_version=2;",
            )
            .map_err(|_| "无法迁移记忆数据库")?;
        transaction.commit().map_err(|_| "无法提交记忆数据库迁移")?;
    }
    Ok(())
}

pub(crate) fn validate_memory(title: &str, content: &str) -> Result<(), String> {
    if title.trim().is_empty()
        || content.trim().is_empty()
        || title.len() > MAX_MEMORY_TITLE_BYTES
        || content.len() > MAX_MEMORY_CONTENT_BYTES
        || title.chars().any(char::is_control)
        || content.chars().any(|c| c == '\0')
    {
        return Err("记忆标题或内容无效；请缩短内容并移除控制字符".into());
    }
    let text = format!("{} {}", title, content).to_lowercase();
    if [
        "密码",
        "口令",
        "身份证",
        "银行卡",
        "支付信息",
        "api key",
        "apikey",
        "password",
        "access token",
        "secret key",
        "private key",
        "credential",
    ]
    .iter()
    .any(|term| text.contains(term))
    {
        return Err("长期记忆不能保存密码、凭证或支付身份信息".into());
    }
    Ok(())
}

fn memories(connection: &Connection) -> Result<Vec<ExplicitMemory>, String> {
    let mut statement = connection.prepare(
        "SELECT id, title, content, created_at, updated_at FROM memories ORDER BY updated_at DESC, id LIMIT ?1"
    ).map_err(|_| "无法读取长期记忆")?;
    let rows = statement
        .query_map([MAX_MEMORIES], |row| {
            Ok(ExplicitMemory {
                id: row.get(0)?,
                title: row.get(1)?,
                content: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })
        .map_err(|_| "无法读取长期记忆")?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| "长期记忆不可读取".into())
}

fn upsert_memory(
    connection: &Connection,
    id: Option<&str>,
    title: &str,
    content: &str,
) -> Result<(), String> {
    validate_memory(title, content)?;
    match id {
        Some(id) => {
            if !valid_run_id(id) {
                return Err("记忆标识无效".into());
            }
            let changed = connection.execute(
                "UPDATE memories SET title=?2, content=?3, updated_at=CURRENT_TIMESTAMP WHERE id=?1 AND source='user_explicit'",
                params![id, title.trim(), content.trim()],
            ).map_err(|_| "无法更新长期记忆")?;
            if changed != 1 {
                return Err("该记忆不存在；请刷新列表".into());
            }
        }
        None => {
            let count: i64 = connection
                .query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))
                .map_err(|_| "无法检查长期记忆容量")?;
            if count >= MAX_MEMORIES {
                return Err("长期记忆已达到 100 条上限；请先整理旧条目".into());
            }
            connection.execute(
                "INSERT INTO memories(id, title, content) VALUES (lower(hex(randomblob(16))), ?1, ?2)",
                params![title.trim(), content.trim()],
            ).map_err(|_| "无法保存长期记忆")?;
        }
    }
    Ok(())
}

fn delete_memory(connection: &Connection, id: &str) -> Result<(), String> {
    if !valid_run_id(id) {
        return Err("记忆标识无效".into());
    }
    let changed = connection
        .execute(
            "DELETE FROM memories WHERE id=?1 AND source='user_explicit'",
            [id],
        )
        .map_err(|_| "无法删除长期记忆")?;
    if changed != 1 {
        return Err("该记忆不存在；请刷新列表".into());
    }
    Ok(())
}

fn open_at(path: &Path) -> Result<Connection, String> {
    let mut connection = Connection::open(path).map_err(|_| "无法打开历史数据库")?;
    initialize(&mut connection)?;
    Ok(connection)
}

fn valid_run_id(run_id: &str) -> bool {
    !run_id.is_empty()
        && run_id.len() <= 100
        && run_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn insert_turn(connection: &mut Connection, turn: &HistoryTurn) -> Result<(), String> {
    if !valid_run_id(&turn.run_id)
        || turn.question.trim().is_empty()
        || turn.question.len() > MAX_QUESTION_BYTES
        || turn.answer.len() > MAX_ANSWER_BYTES
    {
        return Err("历史消息内容或标识无效".into());
    }
    let transaction = connection.transaction().map_err(|_| "无法开始历史事务")?;
    transaction
        .execute(
            "INSERT OR IGNORE INTO conversations(id) VALUES ('desktop-main')",
            [],
        )
        .map_err(|_| "无法建立会话")?;
    for (role, content) in [
        ("user", turn.question.as_str()),
        ("assistant", turn.answer.as_str()),
    ] {
        transaction.execute(
            "INSERT OR IGNORE INTO messages(conversation_id, turn_id, role, content) VALUES ('desktop-main', ?1, ?2, ?3)",
            params![turn.run_id, role, content],
        ).map_err(|_| "无法保存历史消息")?;
    }
    transaction
        .commit()
        .map_err(|_| "无法提交历史消息".to_owned())
}

fn recent_turns(connection: &Connection) -> Result<Vec<HistoryTurn>, String> {
    let mut statement = connection
        .prepare(
            "SELECT u.turn_id, u.content, a.content FROM messages AS u
         JOIN messages AS a ON a.turn_id=u.turn_id AND a.role='assistant'
         WHERE u.role='user' ORDER BY u.id DESC LIMIT ?1",
        )
        .map_err(|_| "无法读取历史消息")?;
    let rows = statement
        .query_map([MAX_RECENT_TURNS], |row| {
            Ok(HistoryTurn {
                run_id: row.get(0)?,
                question: row.get(1)?,
                answer: row.get(2)?,
            })
        })
        .map_err(|_| "无法读取历史消息")?;
    let mut turns = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "历史消息不可读取")?;
    turns.reverse();
    Ok(turns)
}

pub fn may_persist(private_at_start: bool, private_now: bool) -> bool {
    !private_at_start && !private_now
}

impl HistoryStore {
    pub fn open(&self, app: &AppHandle) -> Result<(), String> {
        let dir = app
            .path()
            .app_data_dir()
            .map_err(|_| "无法取得应用数据目录")?;
        fs::create_dir_all(&dir).map_err(|_| "无法建立应用数据目录")?;
        let connection = open_at(&dir.join("phoebe-history.sqlite3"))?;
        *self.0.lock().map_err(|_| "历史数据库状态不可用")? = Some(connection);
        Ok(())
    }

    pub fn recent(&self) -> Result<Vec<HistoryTurn>, String> {
        let guard = self.0.lock().map_err(|_| "历史数据库状态不可用")?;
        recent_turns(
            guard
                .as_ref()
                .ok_or("历史数据库不可用，请检查应用数据目录")?,
        )
    }

    pub fn save(&self, turn: &HistoryTurn) -> Result<(), String> {
        let mut guard = self.0.lock().map_err(|_| "历史数据库状态不可用")?;
        insert_turn(
            guard
                .as_mut()
                .ok_or("历史数据库不可用，请检查应用数据目录")?,
            turn,
        )
    }

    pub fn clear(&self) -> Result<(), String> {
        let guard = self.0.lock().map_err(|_| "历史数据库状态不可用")?;
        guard
            .as_ref()
            .ok_or("历史数据库不可用，请检查应用数据目录")?
            .execute("DELETE FROM messages", [])
            .map_err(|_| "无法清理历史消息")?;
        Ok(())
    }

    pub fn memories(&self) -> Result<Vec<ExplicitMemory>, String> {
        let guard = self.0.lock().map_err(|_| "历史数据库状态不可用")?;
        memories(
            guard
                .as_ref()
                .ok_or("历史数据库不可用，请检查应用数据目录")?,
        )
    }

    pub fn upsert_memory(
        &self,
        id: Option<&str>,
        title: &str,
        content: &str,
    ) -> Result<(), String> {
        let guard = self.0.lock().map_err(|_| "历史数据库状态不可用")?;
        upsert_memory(
            guard
                .as_ref()
                .ok_or("历史数据库不可用，请检查应用数据目录")?,
            id,
            title,
            content,
        )
    }

    pub fn delete_memory(&self, id: &str) -> Result<(), String> {
        let guard = self.0.lock().map_err(|_| "历史数据库状态不可用")?;
        delete_memory(
            guard
                .as_ref()
                .ok_or("历史数据库不可用，请检查应用数据目录")?,
            id,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        delete_memory, initialize, insert_turn, may_persist, memories, open_at, recent_turns,
        upsert_memory, validate_memory, HistoryTurn,
    };
    use rusqlite::Connection;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn migration_and_idempotent_round_trip() {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize(&mut connection).unwrap();
        initialize(&mut connection).unwrap();
        let turn = HistoryTurn {
            run_id: "run-1".into(),
            question: "你好".into(),
            answer: "你好。".into(),
        };
        insert_turn(&mut connection, &turn).unwrap();
        insert_turn(&mut connection, &turn).unwrap();
        assert_eq!(recent_turns(&connection).unwrap(), vec![turn]);
        connection.execute("DELETE FROM messages", []).unwrap();
        assert!(recent_turns(&connection).unwrap().is_empty());
    }

    #[test]
    fn privacy_and_input_validation_fail_closed() {
        assert!(may_persist(false, false));
        assert!(!may_persist(true, false));
        assert!(!may_persist(false, true));
        let mut connection = Connection::open_in_memory().unwrap();
        initialize(&mut connection).unwrap();
        let invalid = HistoryTurn {
            run_id: "../secret".into(),
            question: "你好".into(),
            answer: "回复".into(),
        };
        assert!(insert_turn(&mut connection, &invalid).is_err());
        assert!(recent_turns(&connection).unwrap().is_empty());
    }

    #[test]
    fn file_backed_history_survives_reopen_and_clear() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "phoebe-history-test-{}-{stamp}.sqlite3",
            std::process::id()
        ));
        let turn = HistoryTurn {
            run_id: "persist-1".into(),
            question: "本机吗".into(),
            answer: "是。".into(),
        };
        {
            let mut connection = open_at(&path).unwrap();
            insert_turn(&mut connection, &turn).unwrap();
        }
        {
            let connection = open_at(&path).unwrap();
            assert_eq!(recent_turns(&connection).unwrap(), vec![turn]);
            connection.execute("DELETE FROM messages", []).unwrap();
        }
        assert!(recent_turns(&open_at(&path).unwrap()).unwrap().is_empty());
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn version_one_migration_preserves_messages_and_adds_memories() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
            CREATE TABLE conversations(id TEXT PRIMARY KEY, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
            CREATE TABLE messages(id INTEGER PRIMARY KEY AUTOINCREMENT, conversation_id TEXT NOT NULL, turn_id TEXT NOT NULL, role TEXT NOT NULL, content TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP, UNIQUE(turn_id, role));
            INSERT INTO schema_migrations(version) VALUES (1);
            INSERT INTO conversations(id) VALUES ('desktop-main');
            INSERT INTO messages(conversation_id, turn_id, role, content) VALUES ('desktop-main','old-1','user','旧问题'), ('desktop-main','old-1','assistant','旧回复');
            PRAGMA user_version=1;").unwrap();
        initialize(&mut connection).unwrap();
        initialize(&mut connection).unwrap();
        assert_eq!(recent_turns(&connection).unwrap()[0].answer, "旧回复");
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 2);
        upsert_memory(&connection, None, "语言", "默认使用简体中文").unwrap();
        assert_eq!(memories(&connection).unwrap().len(), 1);
    }

    #[test]
    fn explicit_memory_crud_and_sensitive_input_fail_closed() {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize(&mut connection).unwrap();
        assert!(validate_memory("密码", "请记住").is_err());
        assert!(validate_memory("偏好", "my access token is abc").is_err());
        assert!(validate_memory("", "内容").is_err());
        assert!(memories(&connection).unwrap().is_empty());
        upsert_memory(&connection, None, "语言", "简体中文").unwrap();
        let id = memories(&connection).unwrap()[0].id.clone();
        upsert_memory(&connection, Some(&id), "语言", "繁体中文").unwrap();
        assert_eq!(memories(&connection).unwrap()[0].content, "繁体中文");
        assert!(upsert_memory(&connection, Some("not-found"), "语言", "中文").is_err());
        delete_memory(&connection, &id).unwrap();
        assert!(memories(&connection).unwrap().is_empty());
        assert!(delete_memory(&connection, &id).is_err());
    }
}
