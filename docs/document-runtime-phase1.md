# Document Runtime Phase 1

This phase adds the first local-first document runtime foundation for VisionClip.
It is intentionally small and safe: text and Markdown are supported now, with a
local JSON snapshot store. PDF extraction, OCR, SQLite, vector indexing, and a
controllable audio runtime remain future integration work.

## Scope Delivered

- `visionclip-documents` crate with document IDs, loaders, chunking, reading
  sessions, and a translated reading pipeline.
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
  - `visionclip document read <document_id> --target-lang pt-BR`
  - `visionclip document pause|resume|stop <reading_session_id>`
- Daemon integration with a local snapshot store at the app data directory
  (`documents-store.json`).
- `ask_document` and `summarize_document` executors over local in-memory chunks
  with simple keyword retrieval.
- Initial Ollama/Piper adapters for translated reading:
  `OllamaBackend -> Piper HTTP -> configured audio player`.
- Optional Ollama embedding API foundation via `infer.embedding_model` and
  `/api/embed`; it is disabled by default.
- Best-effort local embedding generation on document ingest when
  `infer.embedding_model` is configured, with lexical fallback if embedding
  generation fails.

## Safety Decisions

- Document ingestion is a level 2 tool and uses `FileRead`.
- Reading and translation are level 2 tools with once-per-resource confirmation.
- Pause, resume, and stop are level 0 audio-control tools.
- PDF paths are rejected with an explicit error until a real extractor is added.
- The runtime does not send content to cloud providers. The daemon adapter uses
  the existing local Ollama backend and Piper HTTP TTS.
- Non-PT-BR document translation is rejected until a generic translation prompt
  or ProviderRouter route exists.

## Current Limitations

- TXT and Markdown only.
- No SQLite store yet.
- No vector index/RAG retrieval yet.
- Document state is persisted locally as JSON. This is a transitional backend;
  SQLite remains the target storage layer.
- Pause/resume/stop update session state, but live cancellation/control of a
  running playback pipeline still needs the AudioRuntime control channel.
- Retrieval uses local embeddings when available for the document and falls
  back to lexical matching otherwise. Vector storage is still the JSON snapshot;
  SQLite vector search is not integrated yet.

## Next Integration Steps

1. Replace the JSON snapshot with local SQLite tables for documents, chunks,
   reading sessions, translations, audio cache, and audit events.
2. Add migrations and store-version handling for SQLite.
3. Add PDF text extraction behind a feature or optional system dependency.
4. Connect translation to ProviderRouter and TTS to a controllable AudioRuntime.
5. Replace in-process embedding ranking with SQLite vector storage/search.
