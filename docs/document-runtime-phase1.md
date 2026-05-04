# Document Runtime Phase 1

This phase adds the first local-first document runtime foundation for VisionClip.
It is intentionally small and safe: text, Markdown, and textual PDFs are
supported now, with local JSON snapshot compatibility plus SQLite persistence.
OCR, sqlite-vec indexing, and a controllable audio runtime remain future
integration work.

## Scope Delivered

- `visionclip-documents` crate with document IDs, TXT/Markdown/PDF loaders,
  chunking, reading sessions, and a translated reading pipeline.
- Bounded realtime pipeline matching the product plan:
  `DocumentChunkProducer -> TranslationWorker -> TtsWorker -> AudioSink`.
- Backpressure defaults:
  - document chunks: 8
  - translated chunks: 8
  - audio chunks: 4
- Tool registry entries for:
  - `ingest_document`
  - `ask_document`
  - `summarize_document`
  - `read_document_aloud`
  - `translate_document`
  - `pause_reading`
  - `resume_reading`
  - `stop_reading`
- Default configuration section for document chunking and cache preferences.
- IPC and CLI commands:
  - `visionclip document ingest <path>`
  - `visionclip document ask <document_id> "<question>"`
  - `visionclip document summarize <document_id>`
  - `visionclip document translate <document_id> --target-lang pt-BR`
  - `visionclip document translate <document_id> --target-lang es`
  - `visionclip document read <document_id> --target-lang pt-BR`
  - `visionclip document pause|resume|stop <reading_session_id>`
- Daemon integration with local document persistence at the app data directory:
  JSON compatibility snapshot (`documents-store.json`) plus SQLite
  (`documents.sqlite3`).
- `ask_document` and `summarize_document` executors over local in-memory chunks
  with simple keyword retrieval.
- Initial Ollama/Piper adapters for translated reading:
  `OllamaBackend -> Piper HTTP -> configured audio player`.
- Optional Ollama embedding API foundation via `infer.embedding_model` and
  `/api/embed`; it is disabled by default.
- Best-effort local embedding generation on document ingest when
  `infer.embedding_model` is configured, with lexical fallback if embedding
  generation fails.
- `SqliteDocumentStore` foundation in `visionclip-documents` with schema
  versioning and tables for documents, chunks, reading sessions, progress,
  translated chunks, chunk embeddings, audio cache entries, and audit events.
- Daemon migration path: when JSON exists it is loaded and mirrored into
  SQLite; when JSON is absent the daemon can load documents, sessions, progress,
  translations, and embeddings from SQLite.
- Daemon audit path: tool/security events are recorded in memory and persisted
  into SQLite with redacted payloads.
- Daemon audio cache path: translated reading writes generated WAV chunks under
  the local app data directory and stores cache metadata in SQLite when
  `documents.cache_audio` is enabled; cache hits are loaded before calling TTS
  again.
- Textual PDF ingestion via optional `pdftotext`/poppler-utils or
  `mutool`/mupdf-tools fallback using fixed process arguments and no shell
  execution.
- Document translation/read target language normalization for `pt-BR`, `en`,
  `es`, `zh`, `ru`, `ja`, `ko`, and `hi` plus common aliases.

## Safety Decisions

- Document ingestion is a level 2 tool and uses `FileRead`.
- Reading and translation are level 2 tools with once-per-resource confirmation.
- Pause, resume, and stop are level 0 audio-control tools.
- Textual PDF extraction uses local `pdftotext` or `mutool`; scanned PDFs still
  require OCR.
- The runtime does not send content to cloud providers. The daemon adapter uses
  the existing local Ollama backend and Piper HTTP TTS.
- Document translation targets are allowlisted; unsupported target names are
  rejected before model calls.

## Current Limitations

- TXT, Markdown, and textual PDFs only.
- SQLite is wired into the daemon as a compatibility mirror and fallback load
  source. JSON remains written during the migration window.
- No vector index/RAG retrieval yet.
- Document state is persisted locally as JSON. This is a transitional backend;
  SQLite remains the target storage layer.
- Pause/resume/stop update session state, but live cancellation/control of a
  running playback pipeline still needs the AudioRuntime control channel.
- Retrieval uses local embeddings when available for the document and falls
  back to lexical matching otherwise. Embeddings are mirrored to SQLite, but
  vector similarity search is still in-process; sqlite-vec is not integrated yet.

## Next Integration Steps

1. Make SQLite the single default document store and remove JSON writes after a
   migration window.
2. Add local OCR fallback for scanned PDFs.
3. Connect translation to ProviderRouter and TTS to a controllable AudioRuntime.
4. Replace in-process embedding ranking with sqlite-vec vector storage/search.
5. Add cache eviction and user-facing reading/cache controls.
