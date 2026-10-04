//! SQLite cache for folders, headers, and message bodies.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::account::{AttachmentMeta, MailFolder, MessageBody, MessageHeader};
use crate::error::Result;

/// Local mail cache (`data/mail/cache.db`).
#[derive(Debug)]
pub struct MailCache {
    conn: Mutex<Connection>,
}

impl MailCache {
    /// Open (creating schema) under `mail_dir`.
    pub fn open(mail_dir: impl AsRef<Path>) -> Result<Arc<Self>> {
        let path: PathBuf = mail_dir.as_ref().join("cache.db");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        conn.execute_batch(
            r#"
            PRAGMA journal_mode=WAL;
            CREATE TABLE IF NOT EXISTS folders (
                account_id TEXT NOT NULL,
                path TEXT NOT NULL,
                name TEXT NOT NULL,
                depth INTEGER NOT NULL,
                role TEXT,
                unread INTEGER NOT NULL,
                total INTEGER NOT NULL,
                PRIMARY KEY (account_id, path)
            );
            CREATE TABLE IF NOT EXISTS headers (
                account_id TEXT NOT NULL,
                folder TEXT NOT NULL,
                uid INTEGER NOT NULL,
                message_id TEXT,
                from_addr TEXT NOT NULL,
                to_addr TEXT NOT NULL,
                subject TEXT NOT NULL,
                date TEXT NOT NULL,
                date_unix INTEGER NOT NULL,
                seen INTEGER NOT NULL,
                flagged INTEGER NOT NULL,
                has_attachment INTEGER NOT NULL,
                snippet TEXT NOT NULL,
                PRIMARY KEY (account_id, folder, uid)
            );
            CREATE INDEX IF NOT EXISTS idx_headers_date
                ON headers(account_id, folder, date_unix DESC);
            CREATE TABLE IF NOT EXISTS bodies (
                account_id TEXT NOT NULL,
                folder TEXT NOT NULL,
                uid INTEGER NOT NULL,
                text TEXT NOT NULL,
                html TEXT NOT NULL,
                attachments_json TEXT NOT NULL,
                PRIMARY KEY (account_id, folder, uid)
            );
            CREATE TABLE IF NOT EXISTS attachment_bytes (
                account_id TEXT NOT NULL,
                folder TEXT NOT NULL,
                uid INTEGER NOT NULL,
                part_id TEXT NOT NULL,
                bytes BLOB NOT NULL,
                PRIMARY KEY (account_id, folder, uid, part_id)
            );
            "#,
        )?;
        Ok(Arc::new(Self {
            conn: Mutex::new(conn),
        }))
    }

    /// Replace folder list for an account.
    pub fn replace_folders(&self, account_id: Uuid, folders: &[MailFolder]) -> Result<()> {
        let conn = self.conn.lock();
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "DELETE FROM folders WHERE account_id = ?1",
            params![account_id.to_string()],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO folders(account_id, path, name, depth, role, unread, total)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            for f in folders {
                stmt.execute(params![
                    account_id.to_string(),
                    f.path,
                    f.name,
                    f.depth,
                    f.role,
                    f.unread,
                    f.total,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// List folders for an account.
    pub fn folders(&self, account_id: Uuid) -> Result<Vec<MailFolder>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT path, name, depth, role, unread, total FROM folders
             WHERE account_id = ?1 ORDER BY path",
        )?;
        let rows = stmt.query_map(params![account_id.to_string()], |row| {
            Ok(MailFolder {
                account_id,
                path: row.get(0)?,
                name: row.get(1)?,
                depth: row.get::<_, i64>(2)? as u32,
                role: row.get(3)?,
                unread: row.get::<_, i64>(4)? as u32,
                total: row.get::<_, i64>(5)? as u32,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Upsert message headers for a folder.
    pub fn upsert_headers(&self, headers: &[MessageHeader]) -> Result<()> {
        let conn = self.conn.lock();
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO headers(
                    account_id, folder, uid, message_id, from_addr, to_addr, subject,
                    date, date_unix, seen, flagged, has_attachment, snippet
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
                 ON CONFLICT(account_id, folder, uid) DO UPDATE SET
                    message_id=excluded.message_id,
                    from_addr=excluded.from_addr,
                    to_addr=excluded.to_addr,
                    subject=excluded.subject,
                    date=excluded.date,
                    date_unix=excluded.date_unix,
                    seen=excluded.seen,
                    flagged=excluded.flagged,
                    has_attachment=excluded.has_attachment,
                    snippet=excluded.snippet",
            )?;
            for h in headers {
                stmt.execute(params![
                    h.account_id.to_string(),
                    h.folder,
                    h.uid,
                    h.message_id,
                    h.from,
                    h.to,
                    h.subject,
                    h.date,
                    h.date_unix,
                    h.seen as i64,
                    h.flagged as i64,
                    h.has_attachment as i64,
                    h.snippet,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Headers for a folder, newest first, limited.
    pub fn headers(
        &self,
        account_id: Uuid,
        folder: &str,
        limit: u32,
    ) -> Result<Vec<MessageHeader>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT uid, message_id, from_addr, to_addr, subject, date, date_unix,
                    seen, flagged, has_attachment, snippet
             FROM headers
             WHERE account_id = ?1 AND folder = ?2
             ORDER BY date_unix DESC, uid DESC
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![account_id.to_string(), folder, limit], |row| {
            Ok(MessageHeader {
                account_id,
                folder: folder.to_string(),
                uid: row.get::<_, i64>(0)? as u32,
                message_id: row.get(1)?,
                from: row.get(2)?,
                to: row.get(3)?,
                subject: row.get(4)?,
                date: row.get(5)?,
                date_unix: row.get(6)?,
                seen: row.get::<_, i64>(7)? != 0,
                flagged: row.get::<_, i64>(8)? != 0,
                has_attachment: row.get::<_, i64>(9)? != 0,
                snippet: row.get(10)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Store a full body and the attachment bytes that came with it.
    pub fn put_body(&self, body: &MessageBody) -> Result<()> {
        let attachments_json = serde_json::to_string(&body.attachments)?;
        let conn = self.conn.lock();
        let tx = conn.unchecked_transaction()?;
        let account = body.account_id.to_string();
        tx.execute(
            "INSERT INTO bodies(account_id, folder, uid, text, html, attachments_json)
             VALUES (?1,?2,?3,?4,?5,?6)
             ON CONFLICT(account_id, folder, uid) DO UPDATE SET
                text=excluded.text,
                html=excluded.html,
                attachments_json=excluded.attachments_json",
            params![
                account,
                body.folder,
                body.uid,
                body.text,
                body.html,
                attachments_json,
            ],
        )?;
        tx.execute(
            "DELETE FROM attachment_bytes WHERE account_id = ?1 AND folder = ?2 AND uid = ?3",
            params![account, body.folder, body.uid],
        )?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO attachment_bytes(account_id, folder, uid, part_id, bytes)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for (meta, bytes) in body.attachments.iter().zip(body.parts.iter()) {
                stmt.execute(params![account, body.folder, body.uid, meta.id, bytes])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Load a cached body.
    pub fn body(&self, account_id: Uuid, folder: &str, uid: u32) -> Result<Option<MessageBody>> {
        let conn = self.conn.lock();
        let row = conn
            .query_row(
                "SELECT text, html, attachments_json FROM bodies
                 WHERE account_id = ?1 AND folder = ?2 AND uid = ?3",
                params![account_id.to_string(), folder, uid],
                |row| {
                    let attachments_json: String = row.get(2)?;
                    let attachments: Vec<AttachmentMeta> =
                        serde_json::from_str(&attachments_json).unwrap_or_default();
                    Ok(MessageBody {
                        account_id,
                        folder: folder.to_string(),
                        uid,
                        text: row.get(0)?,
                        html: row.get(1)?,
                        attachments,
                        parts: Vec::new(),
                    })
                },
            )
            .optional()?;
        let Some(mut body) = row else {
            return Ok(None);
        };
        for meta in &body.attachments {
            let bytes = conn
                .query_row(
                    "SELECT bytes FROM attachment_bytes
                     WHERE account_id = ?1 AND folder = ?2 AND uid = ?3 AND part_id = ?4",
                    params![account_id.to_string(), folder, uid, meta.id],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .optional()?;
            body.parts.push(bytes.unwrap_or_default());
        }
        Ok(Some(body))
    }

    /// Update seen/flagged flags in the header cache.
    pub fn set_flags(
        &self,
        account_id: Uuid,
        folder: &str,
        uid: u32,
        seen: Option<bool>,
        flagged: Option<bool>,
    ) -> Result<()> {
        let conn = self.conn.lock();
        if let Some(seen) = seen {
            conn.execute(
                "UPDATE headers SET seen = ?1 WHERE account_id = ?2 AND folder = ?3 AND uid = ?4",
                params![seen as i64, account_id.to_string(), folder, uid],
            )?;
        }
        if let Some(flagged) = flagged {
            conn.execute(
                "UPDATE headers SET flagged = ?1 WHERE account_id = ?2 AND folder = ?3 AND uid = ?4",
                params![flagged as i64, account_id.to_string(), folder, uid],
            )?;
        }
        Ok(())
    }

    /// Drop all rows for an account.
    pub fn purge_account(&self, account_id: Uuid) -> Result<()> {
        let conn = self.conn.lock();
        let id = account_id.to_string();
        conn.execute("DELETE FROM folders WHERE account_id = ?1", params![id])?;
        conn.execute("DELETE FROM headers WHERE account_id = ?1", params![id])?;
        conn.execute("DELETE FROM bodies WHERE account_id = ?1", params![id])?;
        conn.execute(
            "DELETE FROM attachment_bytes WHERE account_id = ?1",
            params![id],
        )?;
        Ok(())
    }

    /// Remove one message from the cache.
    pub fn delete_message(&self, account_id: Uuid, folder: &str, uid: u32) -> Result<()> {
        let conn = self.conn.lock();
        let id = account_id.to_string();
        conn.execute(
            "DELETE FROM headers WHERE account_id = ?1 AND folder = ?2 AND uid = ?3",
            params![id, folder, uid],
        )?;
        conn.execute(
            "DELETE FROM bodies WHERE account_id = ?1 AND folder = ?2 AND uid = ?3",
            params![id, folder, uid],
        )?;
        conn.execute(
            "DELETE FROM attachment_bytes WHERE account_id = ?1 AND folder = ?2 AND uid = ?3",
            params![id, folder, uid],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::AttachmentMeta;

    #[test]
    fn body_cache_keeps_attachment_bytes() {
        let dir = std::env::temp_dir().join(format!("orchid-mail-att-{}", Uuid::new_v4()));
        let cache = MailCache::open(&dir).unwrap();
        let account = Uuid::new_v4();
        let body = MessageBody {
            account_id: account,
            folder: "INBOX".into(),
            uid: 4,
            text: "hello".into(),
            html: String::new(),
            attachments: vec![AttachmentMeta {
                id: "0".into(),
                filename: "note.txt".into(),
                content_type: "text/plain".into(),
                size: 4,
            }],
            parts: vec![b"note".to_vec()],
        };
        cache.put_body(&body).unwrap();
        let loaded = cache.body(account, "INBOX", 4).unwrap().unwrap();
        assert_eq!(loaded.attachments[0].filename, "note.txt");
        assert_eq!(loaded.parts, vec![b"note".to_vec()]);
        cache.delete_message(account, "INBOX", 4).unwrap();
        assert!(cache.body(account, "INBOX", 4).unwrap().is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}
