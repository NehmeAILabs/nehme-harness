use anyhow::Result;
use rusqlite::Connection;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub id: i64,
    pub source: String,
    pub category: String,
    pub content: String,
    pub rank: f64,
}

#[derive(Debug, Clone)]
pub struct RetrievedOutput {
    pub tool: String,
    pub source: String,
    pub raw_output: String,
    pub bytes: i64,
}

pub struct ContextStore {
    conn: Connection,
}

impl ContextStore {
    pub fn open(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let db_path = data_dir.join("context.db");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
        let mut store = Self { conn };
        store.init_tables()?;
        store.migrate_schema()?;
        Ok(store)
    }

    fn init_tables(&mut self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                timestamp TEXT NOT NULL DEFAULT (datetime('now')),
                category TEXT NOT NULL,
                priority INTEGER NOT NULL DEFAULT 2,
                source TEXT NOT NULL DEFAULT '',
                content TEXT NOT NULL DEFAULT '',
                meta TEXT NOT NULL DEFAULT '{}'
            );
            CREATE INDEX IF NOT EXISTS idx_events_session ON events(session_id);
            CREATE INDEX IF NOT EXISTS idx_events_category ON events(category);
            CREATE INDEX IF NOT EXISTS idx_events_priority ON events(priority);

            CREATE VIRTUAL TABLE IF NOT EXISTS events_fts USING fts5(
                content,
                source,
                category,
                content='events',
                tokenize='porter unicode61'
            );

            CREATE TABLE IF NOT EXISTS tool_output (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                tool TEXT NOT NULL,
                path_or_cmd TEXT NOT NULL DEFAULT '',
                raw_output TEXT NOT NULL,
                summary TEXT NOT NULL DEFAULT '',
                bytes_original INTEGER NOT NULL DEFAULT 0,
                bytes_saved INTEGER NOT NULL DEFAULT 0,
                timestamp TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_tool_output_session ON tool_output(session_id);

            CREATE VIRTUAL TABLE IF NOT EXISTS tool_output_fts USING fts5(
                raw_output,
                summary,
                path_or_cmd,
                content='tool_output',
                tokenize='porter unicode61'
            );

            CREATE TABLE IF NOT EXISTS compact_snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                snapshot TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_compact_snapshots_session ON compact_snapshots(session_id);

            CREATE TRIGGER IF NOT EXISTS events_ai AFTER INSERT ON events BEGIN
                INSERT INTO events_fts(rowid, content, source, category)
                VALUES (new.id, new.content, new.source, new.category);
            END;

            CREATE TRIGGER IF NOT EXISTS events_ad AFTER DELETE ON events BEGIN
                INSERT INTO events_fts(events_fts, rowid, content, source, category)
                VALUES ('delete', old.id, old.content, old.source, old.category);
            END;

            CREATE TRIGGER IF NOT EXISTS tool_output_ai AFTER INSERT ON tool_output BEGIN
                INSERT INTO tool_output_fts(rowid, raw_output, summary, path_or_cmd)
                VALUES (new.id, new.raw_output, new.summary, new.path_or_cmd);
            END;

            CREATE TRIGGER IF NOT EXISTS tool_output_ad AFTER DELETE ON tool_output BEGIN
                INSERT INTO tool_output_fts(tool_output_fts, rowid, raw_output, summary, path_or_cmd)
                VALUES ('delete', old.id, old.raw_output, old.summary, old.path_or_cmd);
            END;
            ",
        )?;
        Ok(())
    }

    fn migrate_schema(&mut self) -> Result<()> {
        let has_trigram_events: bool = self
            .conn
            .query_row(
                "SELECT count(*) > 0 FROM sqlite_master WHERE type='table' AND name='events_fts_trigram'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0) > 0;

        if !has_trigram_events {
            self.conn.execute_batch(
                "CREATE VIRTUAL TABLE IF NOT EXISTS events_fts_trigram USING fts5(
                    content,
                    source,
                    category,
                    content='events',
                    tokenize='trigram'
                );

                CREATE TRIGGER IF NOT EXISTS events_trigram_ai AFTER INSERT ON events BEGIN
                    INSERT INTO events_fts_trigram(rowid, content, source, category)
                    VALUES (new.id, new.content, new.source, new.category);
                END;

                CREATE TRIGGER IF NOT EXISTS events_trigram_ad AFTER DELETE ON events BEGIN
                    INSERT INTO events_fts_trigram(events_fts_trigram, rowid, content, source, category)
                    VALUES ('delete', old.id, old.content, old.source, old.category);
                END;

                CREATE VIRTUAL TABLE IF NOT EXISTS tool_output_fts_trigram USING fts5(
                    raw_output,
                    summary,
                    path_or_cmd,
                    content='tool_output',
                    tokenize='trigram'
                );

                CREATE TRIGGER IF NOT EXISTS tool_output_trigram_ai AFTER INSERT ON tool_output BEGIN
                    INSERT INTO tool_output_fts_trigram(rowid, raw_output, summary, path_or_cmd)
                    VALUES (new.id, new.raw_output, new.summary, new.path_or_cmd);
                END;

                CREATE TRIGGER IF NOT EXISTS tool_output_trigram_ad AFTER DELETE ON tool_output BEGIN
                    INSERT INTO tool_output_fts_trigram(tool_output_fts_trigram, rowid, raw_output, summary, path_or_cmd)
                    VALUES ('delete', old.id, old.raw_output, old.summary, old.path_or_cmd);
                END;

                INSERT INTO events_fts_trigram(rowid, content, source, category)
                    SELECT id, content, source, category FROM events;

                INSERT INTO tool_output_fts_trigram(rowid, raw_output, summary, path_or_cmd)
                    SELECT id, raw_output, summary, path_or_cmd FROM tool_output;
                ",
            )?;
        }

        let has_vocab: bool = self
            .conn
            .query_row(
                "SELECT count(*) > 0 FROM sqlite_master WHERE type='table' AND name='search_vocabulary'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0) > 0;

        if !has_vocab {
            self.conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS search_vocabulary (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    word TEXT NOT NULL UNIQUE,
                    source TEXT NOT NULL DEFAULT ''
                );
                CREATE INDEX IF NOT EXISTS idx_vocab_word ON search_vocabulary(word);
                ",
            )?;
        }

        Ok(())
    }

    pub fn record_event(
        &self,
        session_id: &str,
        category: &str,
        priority: i32,
        source: &str,
        content: &str,
        meta: &str,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO events (session_id, category, priority, source, content, meta)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![session_id, category, priority, source, content, meta],
        )?;

        let words = extract_vocabulary_words(content);
        for word in words {
            let _ = self.conn.execute(
                "INSERT OR IGNORE INTO search_vocabulary (word, source) VALUES (?1, 'events')",
                rusqlite::params![word],
            );
        }

        Ok(self.conn.last_insert_rowid())
    }

    pub fn record_tool_output(
        &self,
        session_id: &str,
        tool: &str,
        path_or_cmd: &str,
        raw_output: &str,
        summary: &str,
        bytes_original: i64,
        bytes_saved: i64,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO tool_output (session_id, tool, path_or_cmd, raw_output, summary, bytes_original, bytes_saved)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![session_id, tool, path_or_cmd, raw_output, summary, bytes_original, bytes_saved],
        )?;

        let words = extract_vocabulary_words(&format!(
            "{} {} {}",
            path_or_cmd,
            summary,
            raw_output.chars().take(5000).collect::<String>()
        ));
        for word in words {
            let _ = self.conn.execute(
                "INSERT OR IGNORE INTO search_vocabulary (word, source) VALUES (?1, 'tool_output')",
                rusqlite::params![word],
            );
        }

        Ok(self.conn.last_insert_rowid())
    }

    pub fn search_events(
        &self,
        session_id: &str,
        query: &str,
        limit: i32,
    ) -> Result<Vec<SearchHit>> {
        let porter_hits = self.search_events_porter(session_id, query, limit)?;
        let trigram_hits = self.search_events_trigram(session_id, query, limit)?;

        if porter_hits.is_empty() && trigram_hits.is_empty() {
            if let Some(corrected) = self.fuzzy_correct(query) {
                let corrected_porter = self.search_events_porter(session_id, &corrected, limit)?;
                let corrected_trigram =
                    self.search_events_trigram(session_id, &corrected, limit)?;
                return Ok(rrf_merge(corrected_porter, corrected_trigram, limit));
            }
            return Ok(Vec::new());
        }

        Ok(rrf_merge(porter_hits, trigram_hits, limit))
    }

    fn search_events_porter(
        &self,
        session_id: &str,
        query: &str,
        limit: i32,
    ) -> Result<Vec<SearchHit>> {
        let sql = "
            SELECT e.id, e.source, e.category, e.content, fts.rank
            FROM events_fts fts
            JOIN events e ON e.id = fts.rowid
            WHERE events_fts MATCH ?1 AND e.session_id = ?2
            ORDER BY fts.rank
            LIMIT ?3
        ";
        let mut stmt = self.conn.prepare(sql)?;
        let hits = stmt
            .query_map(rusqlite::params![query, session_id, limit], |row| {
                Ok(SearchHit {
                    id: row.get(0)?,
                    source: row.get(1)?,
                    category: row.get(2)?,
                    content: row.get(3)?,
                    rank: row.get(4)?,
                })
            })?
            .filter_map(|h| h.ok())
            .collect();
        Ok(hits)
    }

    fn search_events_trigram(
        &self,
        session_id: &str,
        query: &str,
        limit: i32,
    ) -> Result<Vec<SearchHit>> {
        let sql = "
            SELECT e.id, e.source, e.category, e.content, fts.rank
            FROM events_fts_trigram fts
            JOIN events e ON e.id = fts.rowid
            WHERE events_fts_trigram MATCH ?1 AND e.session_id = ?2
            ORDER BY fts.rank
            LIMIT ?3
        ";
        let mut stmt = self.conn.prepare(sql)?;
        let hits = stmt
            .query_map(rusqlite::params![query, session_id, limit], |row| {
                Ok(SearchHit {
                    id: row.get(0)?,
                    source: row.get(1)?,
                    category: row.get(2)?,
                    content: row.get(3)?,
                    rank: row.get(4)?,
                })
            })?
            .filter_map(|h| h.ok())
            .collect();
        Ok(hits)
    }

    pub fn search_tool_output(
        &self,
        session_id: &str,
        query: &str,
        limit: i32,
    ) -> Result<Vec<SearchHit>> {
        let porter_hits = self.search_tool_output_porter(session_id, query, limit)?;
        let trigram_hits = self.search_tool_output_trigram(session_id, query, limit)?;

        if porter_hits.is_empty() && trigram_hits.is_empty() {
            if let Some(corrected) = self.fuzzy_correct(query) {
                let corrected_porter =
                    self.search_tool_output_porter(session_id, &corrected, limit)?;
                let corrected_trigram =
                    self.search_tool_output_trigram(session_id, &corrected, limit)?;
                return Ok(rrf_merge(corrected_porter, corrected_trigram, limit));
            }
            return Ok(Vec::new());
        }

        Ok(rrf_merge(porter_hits, trigram_hits, limit))
    }

    fn search_tool_output_porter(
        &self,
        session_id: &str,
        query: &str,
        limit: i32,
    ) -> Result<Vec<SearchHit>> {
        let sql = "
            SELECT t.id, t.path_or_cmd, t.tool, t.raw_output, fts.rank
            FROM tool_output_fts fts
            JOIN tool_output t ON t.id = fts.rowid
            WHERE tool_output_fts MATCH ?1 AND t.session_id = ?2
            ORDER BY fts.rank
            LIMIT ?3
        ";
        let mut stmt = self.conn.prepare(sql)?;
        let hits = stmt
            .query_map(rusqlite::params![query, session_id, limit], |row| {
                Ok(SearchHit {
                    id: row.get(0)?,
                    source: row.get(1)?,
                    category: row.get(2)?,
                    content: row.get(3)?,
                    rank: row.get(4)?,
                })
            })?
            .filter_map(|h| h.ok())
            .collect();
        Ok(hits)
    }

    fn search_tool_output_trigram(
        &self,
        session_id: &str,
        query: &str,
        limit: i32,
    ) -> Result<Vec<SearchHit>> {
        let sql = "
            SELECT t.id, t.path_or_cmd, t.tool, t.raw_output, fts.rank
            FROM tool_output_fts_trigram fts
            JOIN tool_output t ON t.id = fts.rowid
            WHERE tool_output_fts_trigram MATCH ?1 AND t.session_id = ?2
            ORDER BY fts.rank
            LIMIT ?3
        ";
        let mut stmt = self.conn.prepare(sql)?;
        let hits = stmt
            .query_map(rusqlite::params![query, session_id, limit], |row| {
                Ok(SearchHit {
                    id: row.get(0)?,
                    source: row.get(1)?,
                    category: row.get(2)?,
                    content: row.get(3)?,
                    rank: row.get(4)?,
                })
            })?
            .filter_map(|h| h.ok())
            .collect();
        Ok(hits)
    }

    fn fuzzy_correct(&self, query: &str) -> Option<String> {
        let words: Vec<&str> = query.split_whitespace().collect();
        let mut corrected = String::new();
        let mut any_corrected = false;

        for word in &words {
            if word.len() < 3 {
                if !corrected.is_empty() {
                    corrected.push(' ');
                }
                corrected.push_str(word);
                continue;
            }

            let candidates = self.query_vocabulary_candidates(word);

            let best = candidates
                .iter()
                .filter(|c| {
                    let c_lower = c.to_lowercase();
                    let w_lower = word.to_lowercase();
                    let prefix_len = 3.min(w_lower.len()).min(c_lower.len());
                    c_lower.starts_with(&w_lower[..prefix_len])
                        || w_lower.starts_with(&c_lower[..prefix_len])
                })
                .min_by_key(|c| levenshtein_distance(&c.to_lowercase(), &word.to_lowercase()));

            if let Some(b) = best {
                if b.to_lowercase() != word.to_lowercase() {
                    any_corrected = true;
                }
                if !corrected.is_empty() {
                    corrected.push(' ');
                }
                corrected.push_str(b);
            } else {
                if !corrected.is_empty() {
                    corrected.push(' ');
                }
                corrected.push_str(word);
            }
        }

        if any_corrected { Some(corrected) } else { None }
    }

    fn query_vocabulary_candidates(&self, word: &str) -> Vec<String> {
        let min_len = (word.len() as i32 / 2).max(2);
        let max_len = (word.len() as i32 * 2).max(6);
        let mut stmt = match self.conn.prepare(
            "SELECT word FROM search_vocabulary WHERE length(word) BETWEEN ?1 AND ?2 ORDER BY word LIMIT 100",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let result_iter = match stmt.query_map(rusqlite::params![min_len, max_len], |row| {
            row.get::<_, String>(0)
        }) {
            Ok(iter) => iter,
            Err(_) => return Vec::new(),
        };
        result_iter.filter_map(|r| r.ok()).collect()
    }

    pub fn save_compact_snapshot(&self, session_id: &str, snapshot: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO compact_snapshots (session_id, snapshot) VALUES (?1, ?2)",
            rusqlite::params![session_id, snapshot],
        )?;
        Ok(())
    }

    pub fn load_compact_snapshot(&self, session_id: &str) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT snapshot FROM compact_snapshots WHERE session_id = ?1 ORDER BY created_at DESC LIMIT 1",
        )?;
        let mut rows = stmt.query(rusqlite::params![session_id])?;
        match rows.next()? {
            Some(row) => Ok(Some(row.get(0)?)),
            None => Ok(None),
        }
    }

    /// Retrieve the full raw output of a previously externalized tool result by its rowid.
    pub fn retrieve_tool_output(&self, id: i64) -> Result<Option<RetrievedOutput>> {
        let mut stmt = self.conn.prepare(
            "SELECT tool, path_or_cmd, raw_output, bytes_original FROM tool_output WHERE rowid = ?1",
        )?;
        let mut rows = stmt.query(rusqlite::params![id])?;
        match rows.next()? {
            Some(row) => Ok(Some(RetrievedOutput {
                tool: row.get(0)?,
                source: row.get(1)?,
                raw_output: row.get(2)?,
                bytes: row.get(3)?,
            })),
            None => Ok(None),
        }
    }

    /// Retrieve the most recent externalized output for a session matching a keyword.
    pub fn retrieve_latest_by_keyword(
        &self,
        session_id: &str,
        keyword: &str,
    ) -> Result<Option<RetrievedOutput>> {
        let mut stmt = self.conn.prepare(
            "SELECT tool, path_or_cmd, raw_output, bytes_original
             FROM tool_output
             WHERE session_id = ?1 AND (path_or_cmd LIKE '%' || ?2 || '%' OR summary LIKE '%' || ?2 || '%')
             ORDER BY created_at DESC LIMIT 1",
        )?;
        let mut rows = stmt.query(rusqlite::params![session_id, keyword])?;
        match rows.next()? {
            Some(row) => Ok(Some(RetrievedOutput {
                tool: row.get(0)?,
                source: row.get(1)?,
                raw_output: row.get(2)?,
                bytes: row.get(3)?,
            })),
            None => Ok(None),
        }
    }

    #[allow(dead_code)]
    pub fn cleanup_session(&self, session_id: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM events WHERE session_id = ?1",
            rusqlite::params![session_id],
        )?;
        self.conn.execute(
            "DELETE FROM tool_output WHERE session_id = ?1",
            rusqlite::params![session_id],
        )?;
        self.conn.execute(
            "DELETE FROM compact_snapshots WHERE session_id = ?1",
            rusqlite::params![session_id],
        )?;
        Ok(())
    }

    pub fn context_savings(&self, session_id: &str) -> Result<(i64, i64)> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(SUM(bytes_original), 0), COALESCE(SUM(bytes_saved), 0)
             FROM tool_output WHERE session_id = ?1",
        )?;
        let mut rows = stmt.query(rusqlite::params![session_id])?;
        match rows.next()? {
            Some(row) => Ok((row.get(0)?, row.get(1)?)),
            None => Ok((0, 0)),
        }
    }

    #[allow(dead_code)]
    pub fn expire_old_content(&self, days: i32) -> Result<u64> {
        let cutoff = format!("datetime('now', '-{} days')", days);
        self.conn.execute(
            &format!("DELETE FROM tool_output WHERE timestamp < {cutoff}"),
            [],
        )?;
        let deleted = self.conn.changes() as u64;
        self.conn.execute(
            &format!("DELETE FROM events WHERE timestamp < {cutoff}"),
            [],
        )?;
        Ok(deleted + self.conn.changes() as u64)
    }
}

fn rrf_merge(porter: Vec<SearchHit>, trigram: Vec<SearchHit>, limit: i32) -> Vec<SearchHit> {
    let k: f64 = 60.0;
    let mut scores: std::collections::HashMap<i64, f64> = std::collections::HashMap::new();
    let mut hit_map: std::collections::HashMap<i64, SearchHit> = std::collections::HashMap::new();

    for (rank, hit) in porter.iter().enumerate() {
        let score = 1.0 / (k + rank as f64 + 1.0);
        *scores.entry(hit.id).or_insert(0.0) += score;
        hit_map.entry(hit.id).or_insert_with(|| hit.clone());
    }

    for (rank, hit) in trigram.iter().enumerate() {
        let score = 1.0 / (k + rank as f64 + 1.0);
        *scores.entry(hit.id).or_insert(0.0) += score;
        hit_map.entry(hit.id).or_insert_with(|| hit.clone());
    }

    let mut combined: Vec<(i64, f64)> = scores.into_iter().collect();
    combined.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    combined
        .into_iter()
        .take(limit as usize)
        .filter_map(|(id, score)| {
            hit_map.remove(&id).map(|mut hit| {
                hit.rank = score;
                hit
            })
        })
        .collect()
}

fn extract_vocabulary_words(text: &str) -> Vec<String> {
    let mut words = std::collections::HashSet::new();
    for word in text.split(|c: char| !c.is_alphanumeric() && c != '_') {
        let w = word.trim().to_lowercase();
        if w.len() >= 3 && w.len() <= 50 {
            words.insert(w);
        }
    }
    words.into_iter().take(50).collect()
}

fn levenshtein_distance(a: &str, b: &str) -> i32 {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let a_len = a_chars.len();
    let b_len = b_chars.len();

    if a_len == 0 {
        return b_len as i32;
    }
    if b_len == 0 {
        return a_len as i32;
    }

    let mut matrix = vec![vec![0i32; b_len + 1]; a_len + 1];

    for i in 0..=a_len {
        matrix[i][0] = i as i32;
    }
    for j in 0..=b_len {
        matrix[0][j] = j as i32;
    }

    for i in 1..=a_len {
        for j in 1..=b_len {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            matrix[i][j] = (matrix[i - 1][j] + 1)
                .min(matrix[i][j - 1] + 1)
                .min(matrix[i - 1][j - 1] + cost);
        }
    }

    matrix[a_len][b_len]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein_distance("hello", "hello"), 0);
        assert_eq!(levenshtein_distance("hello", "hallo"), 1);
        assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
        assert_eq!(levenshtein_distance("", "test"), 4);
        assert_eq!(levenshtein_distance("test", ""), 4);
    }

    #[test]
    fn test_extract_vocabulary_words() {
        let words = extract_vocabulary_words("hello world foo_bar baz 123 a b cc");
        assert!(words.contains(&"hello".to_string()));
        assert!(words.contains(&"world".to_string()));
        assert!(words.contains(&"foo_bar".to_string()));
        assert!(!words.contains(&"a".to_string()));
        assert!(!words.contains(&"cc".to_string()));
    }

    #[test]
    fn test_rrf_merge_empty() {
        let result = rrf_merge(vec![], vec![], 5);
        assert!(result.is_empty());
    }

    #[test]
    fn test_rrf_merge_deduplicates() {
        let hit1 = SearchHit {
            id: 1,
            source: "test".to_string(),
            category: "bash".to_string(),
            content: "content".to_string(),
            rank: -1.0,
        };
        let hit2 = SearchHit {
            id: 2,
            source: "test".to_string(),
            category: "bash".to_string(),
            content: "other".to_string(),
            rank: -2.0,
        };
        let porter = vec![hit1.clone(), hit2.clone()];
        let trigram = vec![hit1];
        let result = rrf_merge(porter, trigram, 5);
        assert_eq!(result.len(), 2);
        assert!(result[0].rank > result[1].rank);
    }

    #[test]
    fn search_is_scoped_to_session_and_includes_tool_output_raw_content() {
        let dir = std::env::temp_dir().join(format!(
            "nehme-harness-context-store-test-{}",
            uuid::Uuid::new_v4()
        ));
        let store = ContextStore::open(&dir).unwrap();

        store
            .record_event(
                "session-a",
                "note",
                2,
                "event-source",
                "visible-session-a-needle",
                "{}",
            )
            .unwrap();
        store
            .record_event(
                "session-b",
                "note",
                2,
                "event-source",
                "visible-session-b-needle",
                "{}",
            )
            .unwrap();
        store
            .record_tool_output(
                "session-a",
                "bash",
                "cargo test",
                "rawoutputneedle with full externalized details",
                "summary without the unique raw marker",
                100,
                50,
            )
            .unwrap();

        let event_hits = store
            .search_events("session-a", "visible-session", 10)
            .unwrap();
        assert_eq!(event_hits.len(), 1);
        assert!(event_hits[0].content.contains("session-a"));

        let tool_hits = store
            .search_tool_output("session-a", "rawoutputneedle", 10)
            .unwrap();
        assert_eq!(tool_hits.len(), 1);
        assert!(tool_hits[0].content.contains("full externalized details"));

        std::fs::remove_dir_all(dir).unwrap();
    }
}
