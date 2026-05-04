use rusqlite::Connection;

pub const SEARCH_SCHEMA_VERSION: i64 = 2;

pub fn init_schema(connection: &Connection) -> rusqlite::Result<()> {
    connection.execute_batch(
        r#"
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS search_files (
          file_id INTEGER PRIMARY KEY,
          path TEXT NOT NULL UNIQUE,
          canonical_path TEXT NOT NULL,
          parent_dir TEXT NOT NULL,
          file_name TEXT NOT NULL,
          title TEXT,
          extension TEXT,
          mime TEXT,
          kind TEXT,
          size_bytes INTEGER NOT NULL,
          mtime_ns INTEGER,
          ctime_ns INTEGER,
          inode INTEGER,
          dev INTEGER,
          content_hash TEXT,
          indexed_state TEXT NOT NULL,
          sensitivity TEXT NOT NULL DEFAULT 'normal',
          extractor_version TEXT,
          last_indexed_at INTEGER,
          last_seen_at INTEGER,
          last_error TEXT
        );

        CREATE TABLE IF NOT EXISTS search_chunks (
          chunk_id INTEGER PRIMARY KEY,
          file_id INTEGER NOT NULL,
          chunk_index INTEGER NOT NULL,
          text TEXT NOT NULL,
          text_hash TEXT,
          byte_start INTEGER,
          byte_end INTEGER,
          token_count INTEGER,
          language TEXT,
          source TEXT NOT NULL,
          embedding_id TEXT,
          FOREIGN KEY(file_id) REFERENCES search_files(file_id) ON DELETE CASCADE
        );

        CREATE TABLE IF NOT EXISTS search_jobs (
          job_id INTEGER PRIMARY KEY,
          path TEXT NOT NULL,
          file_id INTEGER,
          job_type TEXT NOT NULL,
          priority INTEGER NOT NULL,
          status TEXT NOT NULL,
          attempts INTEGER NOT NULL DEFAULT 0,
          scheduled_at INTEGER NOT NULL,
          started_at INTEGER,
          finished_at INTEGER,
          last_error TEXT
        );

        CREATE TABLE IF NOT EXISTS search_roots (
          root_id INTEGER PRIMARY KEY,
          path TEXT NOT NULL UNIQUE,
          enabled INTEGER NOT NULL DEFAULT 1,
          added_at INTEGER NOT NULL,
          policy TEXT NOT NULL DEFAULT 'normal'
        );

        CREATE TABLE IF NOT EXISTS search_events (
          event_id INTEGER PRIMARY KEY,
          path TEXT NOT NULL,
          event_kind TEXT NOT NULL,
          observed_at INTEGER NOT NULL,
          coalesced INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS search_usage (
          file_id INTEGER NOT NULL,
          action TEXT NOT NULL,
          count INTEGER NOT NULL DEFAULT 0,
          last_used_at INTEGER,
          PRIMARY KEY(file_id, action)
        );

        CREATE INDEX IF NOT EXISTS idx_search_files_file_name ON search_files(file_name);
        CREATE INDEX IF NOT EXISTS idx_search_files_path ON search_files(path);
        CREATE INDEX IF NOT EXISTS idx_search_files_kind ON search_files(kind);
        CREATE INDEX IF NOT EXISTS idx_search_files_extension ON search_files(extension);
        CREATE INDEX IF NOT EXISTS idx_search_chunks_file_id ON search_chunks(file_id);

        CREATE VIRTUAL TABLE IF NOT EXISTS search_files_fts USING fts5(
          file_name,
          title,
          path,
          kind UNINDEXED,
          extension UNINDEXED,
          tokenize = 'unicode61 remove_diacritics 2',
          prefix = '2 3 4'
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS search_chunks_fts USING fts5(
          text,
          source UNINDEXED,
          tokenize = 'unicode61 remove_diacritics 2',
          prefix = '2 3 4'
        );

        INSERT INTO search_files_fts(rowid, file_name, title, path, kind, extension)
        SELECT file_id, file_name, COALESCE(title, file_name), path, kind, COALESCE(extension, '')
        FROM search_files
        WHERE NOT EXISTS (SELECT 1 FROM search_files_fts LIMIT 1);

        INSERT INTO search_chunks_fts(rowid, text, source)
        SELECT chunk_id, text, source
        FROM search_chunks
        WHERE NOT EXISTS (SELECT 1 FROM search_chunks_fts LIMIT 1);

        PRAGMA user_version = 2;
        "#,
    )
}
