# Assistant Runtime Research - 2026-05-02

This note records current implementation guidance for evolving VisionClip into a
local-first Linux AI assistant. It is based on the repository architecture docs
plus current upstream documentation.

## Runtime Decisions

- Keep Ollama as the default local provider and add provider abstractions around
  existing calls. Ollama now documents `/api/embed` for single and batch text
  embeddings, returning L2-normalized vectors suitable for cosine retrieval.
  Source: https://docs.ollama.com/capabilities/embeddings
- Use structured outputs/tool-calling as a provider capability, never as a trust
  boundary. Tool calls still need local schema validation, risk classification,
  and permission checks before execution.
  Sources: https://docs.ollama.com/capabilities/structured-outputs and
  https://docs.ollama.com/capabilities/tool-calling
- Prefer XDG Desktop Portal for screenshots and screencast on Wayland. The
  portal APIs expose Screenshot requests and ScreenCast sessions whose Start
  response includes PipeWire stream node IDs, which fits a permissioned capture
  flow.
  Sources:
  https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html
  and
  https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.impl.portal.Screenshot.html
- Use PipeWire as the long-term native audio/video substrate. It provides a
  graph-based, low-latency multimedia framework with media negotiation and
  buffer management; current `pw-play`/`paplay` fallbacks remain useful while
  the controllable AudioRuntime is built.
  Source: https://docs.pipewire.org/
- Keep Piper as the local TTS provider for the MVP. The upstream project is a
  local neural TTS stack based on ONNX voices and supports many target
  languages; voice licenses must be surfaced before bundling models.
  Sources: https://github.com/rhasspy/piper and
  https://github.com/OHF-Voice/piper1-gpl
- Use whisper.cpp or a faster-whisper adapter as the initial local STT layer.
  whisper.cpp is suitable for offline Linux operation, has a real-time stream
  example, CPU/GPU paths, and VAD support. Avoid hard-binding the core to one
  engine.
  Source: https://github.com/ggml-org/whisper.cpp
- For VAD, evaluate Silero VAD behind an optional provider. It supports ONNX
  usage, 8 kHz/16 kHz audio, and permissive MIT licensing, but should remain
  optional because it adds model/runtime packaging work.
  Source: https://github.com/snakers4/silero-vad
- For local RAG, target SQLite first and make vector support optional. `sqlite-
  vec` is small, dependency-light, supports Rust packaging, and runs on normal
  desktop Linux, but it is still pre-v1, so keep lexical fallback and migration
  boundaries.
  Source: https://github.com/asg017/sqlite-vec
- For the future settings app, Electron is viable if the renderer is locked
  down. Use `contextIsolation`, no Node integration in renderer, narrow preload
  APIs, and consider Chromium's GlobalShortcutsPortal for Wayland shortcuts.
  Sources: https://www.electronjs.org/docs/latest/tutorial/context-isolation,
  https://www.electronjs.org/docs/latest/tutorial/security, and
  https://www.electronjs.org/docs/latest/api/global-shortcut
- Model settings UI state should be explicit and normalized. React's guidance
  around reducing duplicate state and treating UI transitions as state changes
  maps well to assistant settings, provider status, permissions, and voice
  profiles.
  Source: https://react.dev/learn/managing-state

## Current Gap Priority

1. Persist sessions, audit events, documents, chunks, translations, and reading
   progress in SQLite while preserving the current JSON snapshot migration path.
2. Wire local embeddings into document ingestion and `ask_document` retrieval.
3. Add PDF text extraction behind a safe loader with explicit consent and path
   validation.
4. Build AudioRuntime control channels for pause/resume/stop/skip instead of
   relying only on external player process completion.
5. Move desktop operations into a DesktopController with fixed command
   templates and mockable executors.
6. Add ProviderRouter traits for chat, vision, embedding, STT, translation, and
   TTS while keeping local providers as defaults.
7. Build frontend configuration only on top of daemon IPC; do not duplicate
   privileged logic in the UI process.
