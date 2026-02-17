use rusqlite::{Connection, Result as SqliteResult, params};
use sha2::{Digest, Sha256};
use std::path::Path;
use tracing::info;

pub struct MessageStore {
    conn: Connection,
}

#[derive(Debug)]
pub struct StoredMessage {
    pub uid: String,
    pub message_id: String,
    pub user: String,
    pub date: String,
    pub from_addr: Option<String>,
    pub subject: Option<String>,
    pub plain_text: Option<String>,
    pub html: Option<String>,
    pub has_attachments: bool,
    pub is_processed: bool,
}

#[derive(Debug)]
pub struct StoredAttachment {
    pub id: Option<i64>,
    pub message_uid: String,
    pub filename: String,
    pub attachment_id: Option<String>,
    pub pdf_data: Vec<u8>,
    pub is_processed: bool,
    /// Classification after extraction: "text", "scanned", "error", or "unknown"
    pub content_type: Option<String>,
    /// Extracted plain text (populated only when content_type == "text")
    pub extracted_text: Option<String>,
}

impl MessageStore {
    /// Create a new message store with SQLite backend
    pub fn new<P: AsRef<Path>>(db_path: P) -> SqliteResult<Self> {
        let conn = Connection::open(db_path)?;

        // Create messages table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS messages (
                uid TEXT PRIMARY KEY,
                message_id TEXT NOT NULL,
                user TEXT NOT NULL,
                date TEXT NOT NULL,
                from_addr TEXT,
                subject TEXT,
                plain_text TEXT,
                html TEXT,
                has_attachments INTEGER NOT NULL DEFAULT 0,
                is_processed INTEGER NOT NULL DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // Create attachments table for PDF storage
        conn.execute(
            "CREATE TABLE IF NOT EXISTS attachments (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                message_uid TEXT NOT NULL,
                filename TEXT NOT NULL,
                attachment_id TEXT,
                pdf_data BLOB NOT NULL,
                is_processed INTEGER NOT NULL DEFAULT 0,
                content_type TEXT NOT NULL DEFAULT 'unknown',
                extracted_text TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (message_uid) REFERENCES messages(uid) ON DELETE CASCADE
            )",
            [],
        )?;

        // Create processed_messages table for tracking
        conn.execute(
            "CREATE TABLE IF NOT EXISTS processed_messages (
                uid TEXT PRIMARY KEY,
                processed_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (uid) REFERENCES messages(uid)
            )",
            [],
        )?;

        // Create processed_attachments table for tracking
        conn.execute(
            "CREATE TABLE IF NOT EXISTS processed_attachments (
                attachment_id INTEGER PRIMARY KEY,
                processed_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                FOREIGN KEY (attachment_id) REFERENCES attachments(id)
            )",
            [],
        )?;

        // Create indexes
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_messages_user ON messages(user)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_messages_is_processed ON messages(is_processed)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_messages_date ON messages(date)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_attachments_message_uid ON attachments(message_uid)",
            [],
        )?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_attachments_is_processed ON attachments(is_processed)",
            [],
        )?;

        // Migrate: add content_type and extracted_text columns if missing
        let has_content_type: bool = conn
            .prepare("SELECT content_type FROM attachments LIMIT 0")
            .is_ok();
        if !has_content_type {
            conn.execute_batch(
                "ALTER TABLE attachments ADD COLUMN content_type TEXT;
                 ALTER TABLE attachments ADD COLUMN extracted_text TEXT;",
            )?;
            info!("Migrated attachments table: added content_type, extracted_text");
        }

        info!("Database initialized successfully");
        Ok(Self { conn })
    }

    /// Generate a unique ID from message_id, date, and user
    pub fn generate_uid(message_id: &str, date: &str, user: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(message_id.as_bytes());
        hasher.update(date.as_bytes());
        hasher.update(user.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Insert or update a message
    pub fn upsert_message(&self, msg: &StoredMessage) -> SqliteResult<()> {
        self.conn.execute(
            "INSERT INTO messages 
                (uid, message_id, user, date, from_addr, subject, plain_text, html, has_attachments, is_processed)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(uid) DO UPDATE SET
                from_addr = excluded.from_addr,
                subject = excluded.subject,
                plain_text = excluded.plain_text,
                html = excluded.html,
                has_attachments = excluded.has_attachments",
            params![
                msg.uid,
                msg.message_id,
                msg.user,
                msg.date,
                msg.from_addr,
                msg.subject,
                msg.plain_text,
                msg.html,
                msg.has_attachments,
                msg.is_processed,
            ],
        )?;
        info!(uid = %msg.uid, "Message stored");
        Ok(())
    }

    /// Insert an attachment (PDF)
    pub fn insert_attachment(&self, attachment: &StoredAttachment) -> SqliteResult<i64> {
        self.conn.execute(
            "INSERT INTO attachments 
                (message_uid, filename, attachment_id, pdf_data, is_processed, content_type, extracted_text)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                attachment.message_uid,
                attachment.filename,
                attachment.attachment_id,
                attachment.pdf_data,
                attachment.is_processed,
                attachment.content_type,
                attachment.extracted_text,
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        info!(attachment_id = id, filename = %attachment.filename, "Attachment stored");
        Ok(id)
    }

    /// Mark a message as processed
    pub fn mark_message_as_processed(&self, uid: &str) -> SqliteResult<()> {
        // Update messages table
        self.conn.execute(
            "UPDATE messages SET is_processed = 1 WHERE uid = ?1",
            params![uid],
        )?;

        // Insert into processed_messages table
        self.conn.execute(
            "INSERT OR IGNORE INTO processed_messages (uid) VALUES (?1)",
            params![uid],
        )?;

        info!(uid = %uid, "Message marked as processed");
        Ok(())
    }

    /// Mark an attachment as processed
    pub fn mark_attachment_as_processed(&self, attachment_id: i64) -> SqliteResult<()> {
        // Update attachments table
        self.conn.execute(
            "UPDATE attachments SET is_processed = 1 WHERE id = ?1",
            params![attachment_id],
        )?;

        // Insert into processed_attachments table
        self.conn.execute(
            "INSERT OR IGNORE INTO processed_attachments (attachment_id) VALUES (?1)",
            params![attachment_id],
        )?;

        info!(
            attachment_id = attachment_id,
            "Attachment marked as processed"
        );
        Ok(())
    }

    /// Update an attachment with extraction results and mark it processed.
    pub fn set_attachment_extraction(
        &self,
        attachment_id: i64,
        content_type: &str,
        extracted_text: Option<&str>,
    ) -> SqliteResult<()> {
        self.conn.execute(
            "UPDATE attachments
             SET content_type = ?1, extracted_text = ?2, is_processed = 1
             WHERE id = ?3",
            params![content_type, extracted_text, attachment_id],
        )?;
        self.conn.execute(
            "INSERT OR IGNORE INTO processed_attachments (attachment_id) VALUES (?1)",
            params![attachment_id],
        )?;
        info!(
            attachment_id = attachment_id,
            content_type = content_type,
            "Attachment classified and marked processed"
        );
        Ok(())
    }

    /// Get all attachments that contain extractable text (for heuristic parsing).
    pub fn get_text_attachments(&self) -> SqliteResult<Vec<StoredAttachment>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, message_uid, filename, attachment_id, pdf_data, is_processed, content_type, extracted_text
             FROM attachments
             WHERE content_type = 'text'
             ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| Self::row_to_attachment(row))?;
        rows.collect()
    }

    /// Get all attachments that need OCR (scanned images).
    pub fn get_scanned_attachments(&self) -> SqliteResult<Vec<StoredAttachment>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, message_uid, filename, attachment_id, pdf_data, is_processed, content_type, extracted_text
             FROM attachments
             WHERE content_type = 'scanned'
             ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| Self::row_to_attachment(row))?;
        rows.collect()
    }

    /// Helper: map a row with the 8-column attachment projection to `StoredAttachment`.
    fn row_to_attachment(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredAttachment> {
        Ok(StoredAttachment {
            id: Some(row.get(0)?),
            message_uid: row.get(1)?,
            filename: row.get(2)?,
            attachment_id: row.get(3)?,
            pdf_data: row.get(4)?,
            is_processed: row.get(5)?,
            content_type: row.get(6)?,
            extracted_text: row.get(7)?,
        })
    }

    /// Get all unprocessed messages
    pub fn get_unprocessed_messages(&self) -> SqliteResult<Vec<StoredMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT uid, message_id, user, date, from_addr, subject, plain_text, html, has_attachments, is_processed
             FROM messages
             WHERE is_processed = 0
             ORDER BY date DESC"
        )?;

        let messages = stmt.query_map([], |row| {
            Ok(StoredMessage {
                uid: row.get(0)?,
                message_id: row.get(1)?,
                user: row.get(2)?,
                date: row.get(3)?,
                from_addr: row.get(4)?,
                subject: row.get(5)?,
                plain_text: row.get(6)?,
                html: row.get(7)?,
                has_attachments: row.get(8)?,
                is_processed: row.get(9)?,
            })
        })?;

        messages.collect()
    }

    /// Get all unprocessed PDF attachments (for batch processing)
    pub fn get_unprocessed_attachments(&self) -> SqliteResult<Vec<StoredAttachment>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, message_uid, filename, attachment_id, pdf_data, is_processed, content_type, extracted_text
             FROM attachments
             WHERE is_processed = 0
             ORDER BY created_at DESC",
        )?;

        let attachments = stmt.query_map([], |row| Self::row_to_attachment(row))?;
        attachments.collect()
    }

    /// Get a single attachment by its primary key ID.
    pub fn get_attachment_by_id(&self, id: i64) -> SqliteResult<Option<StoredAttachment>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, message_uid, filename, attachment_id, pdf_data, is_processed, content_type, extracted_text
             FROM attachments
             WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        match rows.next()? {
            Some(row) => Ok(Some(Self::row_to_attachment(row)?)),
            None => Ok(None),
        }
    }

    /// Get all attachments for a specific message
    pub fn get_attachments_for_message(
        &self,
        message_uid: &str,
    ) -> SqliteResult<Vec<StoredAttachment>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, message_uid, filename, attachment_id, pdf_data, is_processed, content_type, extracted_text
             FROM attachments
             WHERE message_uid = ?1
             ORDER BY created_at",
        )?;

        let attachments =
            stmt.query_map(params![message_uid], |row| Self::row_to_attachment(row))?;
        attachments.collect()
    }

    /// Get message by UID
    pub fn get_message_by_uid(&self, uid: &str) -> SqliteResult<Option<StoredMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT uid, message_id, user, date, from_addr, subject, plain_text, html, has_attachments, is_processed
             FROM messages
             WHERE uid = ?1"
        )?;

        let mut rows = stmt.query(params![uid])?;

        if let Some(row) = rows.next()? {
            Ok(Some(StoredMessage {
                uid: row.get(0)?,
                message_id: row.get(1)?,
                user: row.get(2)?,
                date: row.get(3)?,
                from_addr: row.get(4)?,
                subject: row.get(5)?,
                plain_text: row.get(6)?,
                html: row.get(7)?,
                has_attachments: row.get(8)?,
                is_processed: row.get(9)?,
            }))
        } else {
            Ok(None)
        }
    }

    /// Get all processed message UIDs
    pub fn get_processed_uids(&self) -> SqliteResult<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT uid FROM processed_messages ORDER BY processed_at DESC")?;

        let uids = stmt.query_map([], |row| row.get(0))?;
        uids.collect()
    }

    /// Update an attachment's content classification and optionally store extracted text.
    pub fn set_attachment_content(
        &self,
        attachment_id: i64,
        content_type: &str,
        extracted_text: Option<&str>,
    ) -> SqliteResult<()> {
        self.conn.execute(
            "UPDATE attachments SET content_type = ?1, extracted_text = ?2 WHERE id = ?3",
            params![content_type, extracted_text, attachment_id],
        )?;
        info!(
            attachment_id = attachment_id,
            content_type = content_type,
            "Attachment content classified"
        );
        Ok(())
    }

    /// Get attachments by content_type (e.g. "text", "scanned", "error")
    pub fn get_attachments_by_content_type(
        &self,
        content_type: &str,
    ) -> SqliteResult<Vec<StoredAttachment>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, message_uid, filename, attachment_id, pdf_data, is_processed, content_type, extracted_text
             FROM attachments
             WHERE content_type = ?1
             ORDER BY created_at DESC",
        )?;

        let attachments = stmt.query_map(params![content_type], |row| {
            Ok(StoredAttachment {
                id: Some(row.get(0)?),
                message_uid: row.get(1)?,
                filename: row.get(2)?,
                attachment_id: row.get(3)?,
                pdf_data: row.get(4)?,
                is_processed: row.get(5)?,
                content_type: row.get(6)?,
                extracted_text: row.get(7)?,
            })
        })?;

        attachments.collect()
    }

    /// Get count of messages by processing status
    pub fn get_counts(&self) -> SqliteResult<(usize, usize, usize, usize)> {
        let total_messages: usize =
            self.conn
                .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))?;

        let processed_messages: usize = self.conn.query_row(
            "SELECT COUNT(*) FROM messages WHERE is_processed = 1",
            [],
            |row| row.get(0),
        )?;

        let total_attachments: usize =
            self.conn
                .query_row("SELECT COUNT(*) FROM attachments", [], |row| row.get(0))?;

        let processed_attachments: usize = self.conn.query_row(
            "SELECT COUNT(*) FROM attachments WHERE is_processed = 1",
            [],
            |row| row.get(0),
        )?;

        Ok((
            total_messages,
            processed_messages,
            total_attachments,
            processed_attachments,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uid_generation() {
        let uid1 = MessageStore::generate_uid("msg123", "2025-01-01", "user@example.com");
        let uid2 = MessageStore::generate_uid("msg123", "2025-01-01", "user@example.com");
        let uid3 = MessageStore::generate_uid("msg456", "2025-01-01", "user@example.com");

        assert_eq!(uid1, uid2); // Same inputs = same hash
        assert_ne!(uid1, uid3); // Different inputs = different hash
    }
}
