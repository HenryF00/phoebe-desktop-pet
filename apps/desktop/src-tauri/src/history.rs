use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use std::{fs, path::Path, sync::Mutex};
use tauri::{AppHandle, Manager};

use crate::settings::InteractionMode;

fn mode_key(mode: InteractionMode) -> &'static str {
    match mode {
        InteractionMode::Assistant => "assistant",
        InteractionMode::Chat => "chat",
    }
}

fn active_key(mode: InteractionMode) -> String {
    format!("active_conversation:{}", mode_key(mode))
}

const MAX_QUESTION_BYTES: usize = 10_000;
const MAX_ANSWER_BYTES: usize = 1_000_000;
const MAX_RECENT_TURNS: i64 = 50;
const MAX_CONVERSATIONS: i64 = 200;
const MAX_MEMORIES: i64 = 100;
const MAX_MEMORY_TITLE_BYTES: usize = 120;
const MAX_MEMORY_CONTENT_BYTES: usize = 600;
const MAX_CONVERSATION_TITLE_CHARS: usize = 60;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HistoryTurn {
    #[serde(rename = "runId")]
    pub run_id: String,
    pub question: String,
    pub answer: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    #[serde(rename = "turnCount")]
    pub turn_count: i64,
}

/// A user/assistant text pair replayed into the Agent when a conversation is
/// opened. Tool calls and their results are intentionally not persisted.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SeedMessage {
    pub role: &'static str,
    pub content: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ActiveConversation {
    pub conversation: Conversation,
    pub turns: Vec<HistoryTurn>,
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
    if version > 4 {
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
                UNIQUE(conversation_id, turn_id, role)
            );
            CREATE INDEX IF NOT EXISTS messages_recent ON messages(conversation_id, role, id DESC);
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
    if version <= 2 {
        let transaction = connection.transaction().map_err(|_| "无法迁移会话数据库")?;
        transaction
            .execute_batch(
                "ALTER TABLE conversations ADD COLUMN title TEXT NOT NULL DEFAULT '';
            ALTER TABLE conversations ADD COLUMN updated_at TEXT NOT NULL DEFAULT '';
            UPDATE conversations SET updated_at = created_at;
            CREATE TABLE IF NOT EXISTS app_meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
            INSERT OR IGNORE INTO conversations(id, title) VALUES ('desktop-main', '');
            INSERT OR IGNORE INTO app_meta(key, value) VALUES ('active_conversation', 'desktop-main');
            INSERT OR IGNORE INTO schema_migrations(version) VALUES (3);
            PRAGMA user_version=3;",
            )
            .map_err(|_| "无法迁移会话数据库")?;
        transaction.commit().map_err(|_| "无法提交会话数据库迁移")?;
    }
    if version <= 3 {
        let transaction = connection.transaction().map_err(|_| "无法迁移会话模式")?;
        transaction
            .execute_batch(
                "ALTER TABLE conversations ADD COLUMN mode TEXT NOT NULL DEFAULT 'assistant';
            UPDATE app_meta SET key='active_conversation:assistant' WHERE key='active_conversation';
            INSERT OR IGNORE INTO schema_migrations(version) VALUES (4);
            PRAGMA user_version=4;",
            )
            .map_err(|_| "无法迁移会话模式")?;
        transaction.commit().map_err(|_| "无法提交会话模式迁移")?;
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
            if !valid_conversation_id(id) {
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
    if !valid_conversation_id(id) {
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

/// The Agent proposes a preference and the user approves it; storing it as
/// `user_explicit` keeps the same trust level as a manually added memory.
fn remember_preference(connection: &Connection, title: &str, content: &str) -> Result<(), String> {
    validate_memory(title, content)?;
    let title = title.trim();
    let content = content.trim();
    let existing: Option<String> = connection
        .query_row("SELECT id FROM memories WHERE title=?1 LIMIT 1", [title], |row| row.get(0))
        .optional()
        .map_err(|_| "无法读取长期记忆")?;
    match existing {
        Some(id) => {
            connection
                .execute(
                    "UPDATE memories SET content=?2, updated_at=CURRENT_TIMESTAMP WHERE id=?1 AND source='user_explicit'",
                    params![id, content],
                )
                .map_err(|_| "无法更新长期记忆")?;
        }
        None => {
            let count: i64 = connection
                .query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))
                .map_err(|_| "无法检查长期记忆容量")?;
            if count >= MAX_MEMORIES {
                return Err("长期记忆已达到 100 条上限；请先整理旧条目".into());
            }
            connection
                .execute(
                    "INSERT INTO memories(id, title, content) VALUES (lower(hex(randomblob(16))), ?1, ?2)",
                    params![title, content],
                )
                .map_err(|_| "无法保存长期记忆")?;
        }
    }
    Ok(())
}

fn forget_preference(connection: &Connection, title: &str) -> Result<(), String> {
    let title = title.trim();
    if title.is_empty() || title.chars().any(char::is_control) {
        return Err("记忆标题无效".into());
    }
    let changed = connection
        .execute(
            "DELETE FROM memories WHERE title=?1 AND source='user_explicit'",
            [title],
        )
        .map_err(|_| "无法删除长期记忆")?;
    if changed == 0 {
        return Err("没有找到标题匹配的长期记忆".into());
    }
    Ok(())
}

fn memory_by_title(connection: &Connection, title: &str) -> Result<Option<ExplicitMemory>, String> {
    connection
        .query_row(
            "SELECT id, title, content, created_at, updated_at FROM memories WHERE title=?1 ORDER BY updated_at DESC LIMIT 1",
            [title.trim()],
            |row| {
                Ok(ExplicitMemory {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    content: row.get(2)?,
                    created_at: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(|_| "无法读取长期记忆".to_owned())
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

fn valid_conversation_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn set_active(connection: &Connection, mode: InteractionMode, id: &str) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO app_meta(key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![active_key(mode), id],
        )
        .map_err(|_| "无法保存当前会话")?;
    Ok(())
}

fn create_conversation_row(connection: &Connection, mode: InteractionMode) -> Result<String, String> {
    let id: String = connection
        .query_row("SELECT lower(hex(randomblob(16)))", [], |row| row.get(0))
        .map_err(|_| "无法生成会话标识")?;
    connection
        .execute(
            "INSERT INTO conversations(id, title, mode) VALUES (?1, '', ?2)",
            params![id, mode_key(mode)],
        )
        .map_err(|_| "无法新建会话")?;
    Ok(id)
}

fn active_conversation_id(connection: &Connection, mode: InteractionMode) -> Result<String, String> {
    let key = active_key(mode);
    let stored: Option<String> = connection
        .query_row(
            "SELECT value FROM app_meta WHERE key=?1",
            [&key],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| "无法读取当前会话")?;
    if let Some(id) = stored {
        let exists: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM conversations WHERE id=?1 AND mode=?2)",
                params![id, mode_key(mode)],
                |row| row.get(0),
            )
            .map_err(|_| "无法验证当前会话")?;
        if exists {
            return Ok(id);
        }
    }
    let newest: Option<String> = connection
        .query_row(
            "SELECT id FROM conversations WHERE mode=?1 ORDER BY updated_at DESC, created_at DESC, id LIMIT 1",
            [mode_key(mode)],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| "无法选择会话")?;
    let id = match newest {
        Some(id) => id,
        None => create_conversation_row(connection, mode)?,
    };
    set_active(connection, mode, &id)?;
    Ok(id)
}

fn conversation_by_id(connection: &Connection, id: &str) -> Result<Conversation, String> {
    connection
        .query_row(
            "SELECT c.id, c.title, c.updated_at,
                (SELECT COUNT(*) FROM messages m WHERE m.conversation_id=c.id AND m.role='user')
             FROM conversations c WHERE c.id=?1",
            [id],
            |row| {
                Ok(Conversation {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    updated_at: row.get(2)?,
                    turn_count: row.get(3)?,
                })
            },
        )
        .map_err(|_| "该会话不存在".to_owned())
}

fn list_conversations(connection: &Connection, mode: InteractionMode) -> Result<Vec<Conversation>, String> {
    let mut statement = connection
        .prepare(
            "SELECT c.id, c.title, c.updated_at,
                (SELECT COUNT(*) FROM messages m WHERE m.conversation_id=c.id AND m.role='user')
             FROM conversations c
             WHERE c.mode=?1
             ORDER BY c.updated_at DESC, c.created_at DESC, c.id",
        )
        .map_err(|_| "无法读取会话列表")?;
    let rows = statement
        .query_map([mode_key(mode)], |row| {
            Ok(Conversation {
                id: row.get(0)?,
                title: row.get(1)?,
                updated_at: row.get(2)?,
                turn_count: row.get(3)?,
            })
        })
        .map_err(|_| "无法读取会话列表")?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|_| "会话列表不可读取".into())
}

fn derive_title(question: &str) -> String {
    let cleaned = question.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut title: String = cleaned.chars().take(MAX_CONVERSATION_TITLE_CHARS).collect();
    if cleaned.chars().count() > MAX_CONVERSATION_TITLE_CHARS {
        title.push('…');
    }
    if title.trim().is_empty() {
        "新对话".to_owned()
    } else {
        title
    }
}

fn insert_turn(connection: &mut Connection, mode: InteractionMode, turn: &HistoryTurn) -> Result<(), String> {
    if !valid_run_id(&turn.run_id)
        || turn.question.trim().is_empty()
        || turn.question.len() > MAX_QUESTION_BYTES
        || turn.answer.len() > MAX_ANSWER_BYTES
    {
        return Err("历史消息内容或标识无效".into());
    }
    let active = active_conversation_id(connection, mode)?;
    let transaction = connection.transaction().map_err(|_| "无法开始历史事务")?;
    for (role, content) in [
        ("user", turn.question.as_str()),
        ("assistant", turn.answer.as_str()),
    ] {
        transaction.execute(
            "INSERT OR IGNORE INTO messages(conversation_id, turn_id, role, content) VALUES (?1, ?2, ?3, ?4)",
            params![active, turn.run_id, role, content],
        ).map_err(|_| "无法保存历史消息")?;
    }
    transaction
        .execute(
            "UPDATE conversations SET updated_at=CURRENT_TIMESTAMP WHERE id=?1",
            [&active],
        )
        .map_err(|_| "无法更新会话时间")?;
    let title: String = transaction
        .query_row(
            "SELECT title FROM conversations WHERE id=?1",
            [&active],
            |row| row.get(0),
        )
        .map_err(|_| "无法读取会话标题")?;
    if title.trim().is_empty() {
        transaction
            .execute(
                "UPDATE conversations SET title=?2 WHERE id=?1",
                params![active, derive_title(&turn.question)],
            )
            .map_err(|_| "无法更新会话标题")?;
    }
    transaction
        .commit()
        .map_err(|_| "无法提交历史消息".to_owned())
}

fn recent_turns(connection: &Connection, mode: InteractionMode) -> Result<Vec<HistoryTurn>, String> {
    let active = active_conversation_id(connection, mode)?;
    let mut statement = connection
        .prepare(
            "SELECT u.turn_id, u.content, a.content FROM messages AS u
         JOIN messages AS a ON a.turn_id=u.turn_id AND a.role='assistant'
         WHERE u.role='user' AND u.conversation_id=?1
         ORDER BY u.id DESC LIMIT ?2",
        )
        .map_err(|_| "无法读取历史消息")?;
    let rows = statement
        .query_map(params![active, MAX_RECENT_TURNS], |row| {
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

/// Upper bound for the replayed seed, kept well below the sidecar's 64 KiB
/// command-line cap so long conversations still re-seed reliably.
const MAX_SEED_BYTES: usize = 40_000;
const MAX_SEED_MESSAGE_BYTES: usize = 12_000;

fn seed_content(content: &str) -> String {
    if content.len() <= MAX_SEED_MESSAGE_BYTES {
        content.to_owned()
    } else {
        let mut end = MAX_SEED_MESSAGE_BYTES;
        while !content.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}\n…（更早内容已截断）", &content[..end])
    }
}

/// Text-only replay of the active conversation, oldest first. Keeps the most
/// recent turns and truncates oversized messages so the seed stays bounded.
fn conversation_seed(connection: &Connection, mode: InteractionMode) -> Result<Vec<SeedMessage>, String> {
    let turns = recent_turns(connection, mode)?;
    let mut retained: Vec<(String, String)> = Vec::new();
    let mut total = 0usize;
    for turn in turns.iter().rev() {
        let question = seed_content(&turn.question);
        let answer = seed_content(&turn.answer);
        let pair_bytes = question.len() + answer.len();
        if total + pair_bytes > MAX_SEED_BYTES && !retained.is_empty() {
            break;
        }
        retained.push((question, answer));
        total += pair_bytes;
    }
    let mut seed = Vec::with_capacity(retained.len() * 2);
    for (question, answer) in retained.into_iter().rev() {
        seed.push(SeedMessage {
            role: "user",
            content: question,
        });
        seed.push(SeedMessage {
            role: "assistant",
            content: answer,
        });
    }
    Ok(seed)
}

fn create_conversation(connection: &Connection, mode: InteractionMode) -> Result<Conversation, String> {
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM conversations", [], |row| row.get(0))
        .map_err(|_| "无法检查会话容量")?;
    if count >= MAX_CONVERSATIONS {
        return Err("会话数量已达上限；请先删除旧会话".into());
    }
    let id = create_conversation_row(connection, mode)?;
    set_active(connection, mode, &id)?;
    conversation_by_id(connection, &id)
}

fn switch_conversation(connection: &Connection, mode: InteractionMode, id: &str) -> Result<(), String> {
    if !valid_conversation_id(id) {
        return Err("会话标识无效".into());
    }
    let exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM conversations WHERE id=?1 AND mode=?2)",
            params![id, mode_key(mode)],
            |row| row.get(0),
        )
        .map_err(|_| "无法验证会话")?;
    if !exists {
        return Err("该会话不存在".into());
    }
    set_active(connection, mode, id)
}

/// Deletes a conversation and returns the id that is active afterwards.
fn delete_conversation(connection: &Connection, mode: InteractionMode, id: &str) -> Result<String, String> {
    if !valid_conversation_id(id) {
        return Err("会话标识无效".into());
    }
    let active = active_conversation_id(connection, mode)?;
    let changed = connection
        .execute("DELETE FROM conversations WHERE id=?1 AND mode=?2", params![id, mode_key(mode)])
        .map_err(|_| "无法删除会话")?;
    if changed == 0 {
        return Err("该会话不存在".into());
    }
    if active != id {
        return Ok(active);
    }
    let next: Option<String> = connection
        .query_row(
            "SELECT id FROM conversations WHERE mode=?1 ORDER BY updated_at DESC, created_at DESC, id LIMIT 1",
            [mode_key(mode)],
            |row| row.get(0),
        )
        .optional()
        .map_err(|_| "无法选择会话")?;
    let next = match next {
        Some(id) => id,
        None => create_conversation_row(connection, mode)?,
    };
    set_active(connection, mode, &next)?;
    Ok(next)
}

fn rename_conversation(connection: &Connection, id: &str, title: &str) -> Result<(), String> {
    if !valid_conversation_id(id) {
        return Err("会话标识无效".into());
    }
    let title = title.trim();
    if title.is_empty()
        || title.chars().count() > MAX_CONVERSATION_TITLE_CHARS
        || title.chars().any(char::is_control)
    {
        return Err("会话标题无效".into());
    }
    let changed = connection
        .execute(
            "UPDATE conversations SET title=?2 WHERE id=?1",
            params![id, title],
        )
        .map_err(|_| "无法重命名会话")?;
    if changed == 0 {
        return Err("该会话不存在".into());
    }
    Ok(())
}

/// Clears every message of the active conversation while keeping the
/// conversation itself, resetting its auto-derived title so the next first
/// question can title it again.
fn clear_conversation(connection: &Connection, mode: InteractionMode) -> Result<(), String> {
    let active = active_conversation_id(connection, mode)?;
    connection
        .execute("DELETE FROM messages WHERE conversation_id=?1", [&active])
        .map_err(|_| "无法清理会话消息")?;
    connection
        .execute("UPDATE conversations SET title='' WHERE id=?1", [&active])
        .map_err(|_| "无法重置会话标题")?;
    Ok(())
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

    fn with<T>(&self, action: impl FnOnce(&Connection) -> Result<T, String>) -> Result<T, String> {
        let guard = self.0.lock().map_err(|_| "历史数据库状态不可用")?;
        action(
            guard
                .as_ref()
                .ok_or("历史数据库不可用，请检查应用数据目录")?,
        )
    }

    pub fn recent(&self, mode: InteractionMode) -> Result<Vec<HistoryTurn>, String> {
        self.with(|connection| recent_turns(connection, mode))
    }

    pub fn active(&self, mode: InteractionMode) -> Result<ActiveConversation, String> {
        self.with(|connection| {
            let id = active_conversation_id(connection, mode)?;
            Ok(ActiveConversation {
                conversation: conversation_by_id(connection, &id)?,
                turns: recent_turns(connection, mode)?,
            })
        })
    }

    pub fn active_id(&self, mode: InteractionMode) -> Result<String, String> {
        self.with(|connection| active_conversation_id(connection, mode))
    }

    pub fn seed(&self, mode: InteractionMode) -> Result<Vec<SeedMessage>, String> {
        self.with(|connection| conversation_seed(connection, mode))
    }

    pub fn list(&self, mode: InteractionMode) -> Result<Vec<Conversation>, String> {
        self.with(|connection| list_conversations(connection, mode))
    }

    pub fn create(&self, mode: InteractionMode) -> Result<ActiveConversation, String> {
        self.with(|connection| {
            let conversation = create_conversation(connection, mode)?;
            Ok(ActiveConversation {
                conversation,
                turns: Vec::new(),
            })
        })
    }

    pub fn switch(&self, mode: InteractionMode, id: &str) -> Result<ActiveConversation, String> {
        self.with(|connection| {
            switch_conversation(connection, mode, id)?;
            Ok(ActiveConversation {
                conversation: conversation_by_id(connection, id)?,
                turns: recent_turns(connection, mode)?,
            })
        })
    }

    pub fn delete(&self, mode: InteractionMode, id: &str) -> Result<ActiveConversation, String> {
        self.with(|connection| {
            let next = delete_conversation(connection, mode, id)?;
            Ok(ActiveConversation {
                conversation: conversation_by_id(connection, &next)?,
                turns: recent_turns(connection, mode)?,
            })
        })
    }

    pub fn rename(&self, id: &str, title: &str) -> Result<(), String> {
        self.with(|connection| rename_conversation(connection, id, title))
    }

    pub fn save(&self, mode: InteractionMode, turn: &HistoryTurn) -> Result<(), String> {
        let mut guard = self.0.lock().map_err(|_| "历史数据库状态不可用")?;
        insert_turn(
            guard
                .as_mut()
                .ok_or("历史数据库不可用，请检查应用数据目录")?,
            mode,
            turn,
        )
    }

    pub fn clear(&self, mode: InteractionMode) -> Result<(), String> {
        self.with(|connection| clear_conversation(connection, mode))
    }

    pub fn memories(&self) -> Result<Vec<ExplicitMemory>, String> {
        self.with(memories)
    }

    pub fn upsert_memory(
        &self,
        id: Option<&str>,
        title: &str,
        content: &str,
    ) -> Result<(), String> {
        self.with(|connection| upsert_memory(connection, id, title, content))
    }

    pub fn delete_memory(&self, id: &str) -> Result<(), String> {
        self.with(|connection| delete_memory(connection, id))
    }

    /// Agent-proposed preference storage: update by title or insert new.
    pub fn remember_preference(&self, title: &str, content: &str) -> Result<(), String> {
        self.with(|connection| remember_preference(connection, title, content))
    }

    /// Agent-proposed preference removal, matched by title.
    pub fn forget_preference(&self, title: &str) -> Result<(), String> {
        self.with(|connection| forget_preference(connection, title))
    }

    pub fn memory_by_title(&self, title: &str) -> Result<Option<ExplicitMemory>, String> {
        self.with(|connection| memory_by_title(connection, title))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        active_conversation_id, clear_conversation, conversation_seed, create_conversation,
        delete_conversation, delete_memory, forget_preference, initialize, insert_turn,
        list_conversations, may_persist, memories, open_at, recent_turns, remember_preference,
        rename_conversation, switch_conversation, upsert_memory, validate_memory, HistoryTurn,
        InteractionMode,
    };
    use rusqlite::Connection;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn turn(run_id: &str, question: &str, answer: &str) -> HistoryTurn {
        HistoryTurn {
            run_id: run_id.into(),
            question: question.into(),
            answer: answer.into(),
        }
    }

    #[test]
    fn migration_and_idempotent_round_trip() {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize(&mut connection).unwrap();
        initialize(&mut connection).unwrap();
        let only = turn("run-1", "你好", "你好。");
        insert_turn(&mut connection, InteractionMode::Assistant, &only).unwrap();
        insert_turn(&mut connection, InteractionMode::Assistant, &only).unwrap();
        assert_eq!(recent_turns(&connection, InteractionMode::Assistant).unwrap(), vec![only]);
        assert_eq!(list_conversations(&connection, InteractionMode::Assistant).unwrap()[0].title, "你好");
    }

    #[test]
    fn conversations_are_isolated_and_switchable() {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize(&mut connection).unwrap();
        insert_turn(&mut connection, InteractionMode::Assistant, &turn("run-1", "第一问", "第一答")).unwrap();
        let second = create_conversation(&connection, InteractionMode::Assistant).unwrap();
        assert_eq!(second.title, "");
        assert!(recent_turns(&connection, InteractionMode::Assistant).unwrap().is_empty());
        insert_turn(&mut connection, InteractionMode::Assistant, &turn("run-2", "第二问", "第二答")).unwrap();
        let conversations = list_conversations(&connection, InteractionMode::Assistant).unwrap();
        assert_eq!(conversations.len(), 2);

        // Switching back restores the first conversation and its seed.
        let first = conversations
            .iter()
            .find(|conversation| conversation.title == "第一问")
            .unwrap()
            .id
            .clone();
        switch_conversation(&connection, InteractionMode::Assistant, &first).unwrap();
        assert_eq!(active_conversation_id(&connection, InteractionMode::Assistant).unwrap(), first);
        assert_eq!(recent_turns(&connection, InteractionMode::Assistant).unwrap()[0].question, "第一问");
        let seed = conversation_seed(&connection, InteractionMode::Assistant).unwrap();
        assert_eq!(seed.len(), 2);
        assert_eq!(seed[0].content, "第一问");
        assert_eq!(seed[1].role, "assistant");

        // Deleting the active conversation falls back to the remaining one.
        let remaining = delete_conversation(&connection, InteractionMode::Assistant, &first).unwrap();
        assert_ne!(remaining, first);
        assert_eq!(list_conversations(&connection, InteractionMode::Assistant).unwrap().len(), 1);
    }

    #[test]
    fn deleting_the_last_conversation_creates_a_fresh_one() {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize(&mut connection).unwrap();
        let id = active_conversation_id(&connection, InteractionMode::Assistant).unwrap();
        let next = delete_conversation(&connection, InteractionMode::Assistant, &id).unwrap();
        assert_ne!(next, id);
        assert!(recent_turns(&connection, InteractionMode::Assistant).unwrap().is_empty());
    }

    #[test]
    fn conversations_are_isolated_by_mode() {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize(&mut connection).unwrap();
        insert_turn(&mut connection, InteractionMode::Assistant, &turn("a-1", "助手问题", "助手回答")).unwrap();
        // Chat mode starts with its own empty conversation, not the assistant one.
        assert!(recent_turns(&connection, InteractionMode::Chat).unwrap().is_empty());
        insert_turn(&mut connection, InteractionMode::Chat, &turn("c-1", "聊天问题", "聊天回答")).unwrap();

        assert_eq!(list_conversations(&connection, InteractionMode::Assistant).unwrap().len(), 1);
        assert_eq!(list_conversations(&connection, InteractionMode::Chat).unwrap().len(), 1);
        assert_eq!(recent_turns(&connection, InteractionMode::Assistant).unwrap()[0].question, "助手问题");
        assert_eq!(recent_turns(&connection, InteractionMode::Chat).unwrap()[0].question, "聊天问题");

        let assistant_id = active_conversation_id(&connection, InteractionMode::Assistant).unwrap();
        let chat_id = active_conversation_id(&connection, InteractionMode::Chat).unwrap();
        assert_ne!(assistant_id, chat_id);
        let chat_seed = conversation_seed(&connection, InteractionMode::Chat).unwrap();
        assert_eq!(chat_seed[0].content, "聊天问题");
    }

    #[test]
    fn clearing_keeps_the_conversation_and_clears_messages() {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize(&mut connection).unwrap();
        let id = active_conversation_id(&connection, InteractionMode::Assistant).unwrap();
        insert_turn(&mut connection, InteractionMode::Assistant, &turn("run-1", "第一问", "第一答")).unwrap();
        assert_eq!(list_conversations(&connection, InteractionMode::Assistant).unwrap()[0].title, "第一问");
        clear_conversation(&connection, InteractionMode::Assistant).unwrap();
        assert_eq!(active_conversation_id(&connection, InteractionMode::Assistant).unwrap(), id);
        assert!(recent_turns(&connection, InteractionMode::Assistant).unwrap().is_empty());
        assert_eq!(conversation_seed(&connection, InteractionMode::Assistant).unwrap().len(), 0);
        assert_eq!(list_conversations(&connection, InteractionMode::Assistant).unwrap()[0].title, "");
        // A fresh turn can title the conversation again.
        insert_turn(&mut connection, InteractionMode::Assistant, &turn("run-2", "第二问", "第二答")).unwrap();
        assert_eq!(list_conversations(&connection, InteractionMode::Assistant).unwrap()[0].title, "第二问");
    }

    #[test]
    fn seed_stays_bounded_and_keeps_the_newest_turns() {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize(&mut connection).unwrap();
        // Many turns so the raw total would exceed the seed budget.
        for index in 0..40 {
            let run_id = format!("run-{index}");
            insert_turn(
                &mut connection,
                InteractionMode::Assistant,
                &turn(&run_id, &format!("问题{index}"), &"答".repeat(1_500)),
            )
            .unwrap();
        }
        let seed = conversation_seed(&connection, InteractionMode::Assistant).unwrap();
        let total: usize = seed.iter().map(|message| message.content.len()).sum();
        assert!(total <= super::MAX_SEED_BYTES + super::MAX_SEED_MESSAGE_BYTES * 2);
        assert_eq!(seed[0].role, "user");
        assert!(seed[seed.len() - 1].content.starts_with("答答答"));
        // The oldest turns were dropped; the newest question is still present.
        assert!(seed.iter().any(|message| message.content == "问题39"));
        assert!(!seed.iter().any(|message| message.content == "问题0"));
    }

    #[test]
    fn conversation_titles_are_bounded_and_renameable() {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize(&mut connection).unwrap();
        let id = active_conversation_id(&connection, InteractionMode::Assistant).unwrap();
        assert!(rename_conversation(&connection, &id, "  新标题  ").is_ok());
        assert_eq!(list_conversations(&connection, InteractionMode::Assistant).unwrap()[0].title, "新标题");
        assert!(rename_conversation(&connection, &id, "   ").is_err());
        assert!(rename_conversation(&connection, &id, &"x".repeat(61)).is_err());
        insert_turn(&mut connection, InteractionMode::Assistant, &turn("run-1", "这是一个很长的首个问题用来生成标题", "答")).unwrap();
        assert_eq!(list_conversations(&connection, InteractionMode::Assistant).unwrap()[0].title, "新标题");
    }

    #[test]
    fn privacy_and_input_validation_fail_closed() {
        assert!(may_persist(false, false));
        assert!(!may_persist(true, false));
        assert!(!may_persist(false, true));
        let mut connection = Connection::open_in_memory().unwrap();
        initialize(&mut connection).unwrap();
        let invalid = turn("../secret", "你好", "回复");
        assert!(insert_turn(&mut connection, InteractionMode::Assistant, &invalid).is_err());
        assert!(recent_turns(&connection, InteractionMode::Assistant).unwrap().is_empty());
    }

    #[test]
    fn file_backed_history_survives_reopen_and_delete() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "phoebe-history-test-{}-{stamp}.sqlite3",
            std::process::id()
        ));
        let only = turn("persist-1", "本机吗", "是。");
        {
            let mut connection = open_at(&path).unwrap();
            insert_turn(&mut connection, InteractionMode::Assistant, &only).unwrap();
        }
        {
            let connection = open_at(&path).unwrap();
            assert_eq!(recent_turns(&connection, InteractionMode::Assistant).unwrap(), vec![only]);
            let id = active_conversation_id(&connection, InteractionMode::Assistant).unwrap();
            delete_conversation(&connection, InteractionMode::Assistant, &id).unwrap();
        }
        assert!(recent_turns(&open_at(&path).unwrap(), InteractionMode::Assistant).unwrap().is_empty());
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn version_one_migration_preserves_messages_and_adds_memories() {
        let mut connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
            CREATE TABLE conversations(id TEXT PRIMARY KEY, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
            CREATE TABLE messages(id INTEGER PRIMARY KEY AUTOINCREMENT, conversation_id TEXT NOT NULL, turn_id TEXT NOT NULL, role TEXT NOT NULL, content TEXT NOT NULL, created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP, UNIQUE(conversation_id, turn_id, role));
            INSERT INTO schema_migrations(version) VALUES (1);
            INSERT INTO conversations(id) VALUES ('desktop-main');
            INSERT INTO messages(conversation_id, turn_id, role, content) VALUES ('desktop-main','old-1','user','旧问题'), ('desktop-main','old-1','assistant','旧回复');
            PRAGMA user_version=1;").unwrap();
        initialize(&mut connection).unwrap();
        initialize(&mut connection).unwrap();
        assert_eq!(recent_turns(&connection, InteractionMode::Assistant).unwrap()[0].answer, "旧回复");
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, 4);
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

    #[test]
    fn agent_preferences_upsert_by_title_and_forget() {
        let mut connection = Connection::open_in_memory().unwrap();
        initialize(&mut connection).unwrap();
        remember_preference(&connection, "语言", "简体中文").unwrap();
        assert_eq!(memories(&connection).unwrap()[0].content, "简体中文");
        // Same title updates instead of duplicating.
        remember_preference(&connection, "语言", "繁体中文").unwrap();
        assert_eq!(memories(&connection).unwrap().len(), 1);
        assert_eq!(memories(&connection).unwrap()[0].content, "繁体中文");
        forget_preference(&connection, "语言").unwrap();
        assert!(memories(&connection).unwrap().is_empty());
        assert!(forget_preference(&connection, "语言").is_err());
    }
}
