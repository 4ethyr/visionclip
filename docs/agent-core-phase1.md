# VisionClip Agent Core Phase 1

## Arquitetura atual detectada

O fluxo nativo permanece:

```text
visionclip CLI -> Unix Socket -> visionclip-daemon -> inferencia/acoes/saidas
```

Principais módulos atuais:

- `apps/visionclip`: CLI fino, captura, voz one-shot e envio de `VisionRequest`.
- `apps/visionclip-daemon`: runtime residente, dispatch de jobs, Ollama, busca, clipboard, TTS e abertura de apps/URLs.
- `crates/common`: IPC, config, intents e registry declarativo de ações.
- `crates/infer`: `OllamaBackend` e prompts.
- `crates/output`: clipboard, browser e notificação.
- `crates/tts`: Piper HTTP e playback por player externo.

## Gaps principais

- O daemon ainda não tem loop agentic multi-step com tool result retornando ao modelo.
- Provider router ainda não existe; Ollama segue acoplado em `visionclip-infer`.
- Session manager e audit log ainda são MVPs em memória.
- Não existe fluxo de confirmação UI/IPC para ações que exigem confirmação.
- Document runtime agora tem uma base Phase 1 com IPC/CLI/daemon e snapshot JSON local; PDF, RAG, SQLite e audio runtime controlável ainda estão pendentes.

## Fase 1 implementada

- `ToolRegistry` declarativo com validação de schema JSON para tools existentes.
- `PermissionEngine` básico com risco 0-5, confirmação para risco >= 3, bloqueio de risco 5, shell arbitrário e cloud em contexto sensível.
- `SessionManager` básico com contexto, histórico curto, documento atual e expiração.
- `AuditLog` básico em memória com redaction de campos sensíveis.
- `AgentOrchestrator` mínimo com planejamento determinístico para comandos simples e validação de tool calls.
- O daemon agora valida e audita `open_application`, `open_url`, `search_web` e `capture_screen_context` antes da execução nativa.

## Revisão e continuação

- `AgentTurn.policy` agora influencia a avaliação de cada turno.
- `open_url` é bloqueado pelo `PermissionEngine` quando a URL não é `http://`/`https://`, contém whitespace/control chars ou usa esquemas como `javascript:`/`file:`.
- `set_volume`, `set_brightness` e `toggle_vpn` foram registrados como tools de risco 3. Elas exigem confirmação e ainda não possuem executor nativo nesta fase.
- Alterações de rede/VPN sempre exigem confirmação, mesmo quando o contexto é iniciado pelo usuário.
- `visionclip-documents` adiciona loader TXT/Markdown/PDF textual, chunker, sessão de leitura e pipeline incremental traduzir -> TTS -> áudio com backpressure.
- `ingest_document`, `ask_document`, `summarize_document`, `read_document_aloud`, `translate_document` e controles de leitura já passam por IPC/CLI/daemon com validação de tool e store local persistido.
- `translate_document` e `read_document_aloud` aceitam alvos allowlisted `pt-BR`, `en`, `es`, `zh`, `ru`, `ja`, `ko` e `hi`; idiomas desconhecidos são rejeitados antes de chamar o modelo.
- `ask_document` usa embeddings locais via Ollama quando `infer.embedding_model` esta configurado e ha vetores persistidos para o documento; caso contrario volta para recuperação lexical. `summarize_document` ainda usa prefixo local.
- `visionclip-documents` agora tem `SqliteDocumentStore` com schema versionado para documentos, chunks, sessões, progresso, traduções, embeddings, cache de áudio e eventos de auditoria; o daemon espelha o snapshot JSON para SQLite e consegue recarregar do SQLite quando o JSON nao existe.
- Eventos de auditoria de tools/security continuam em memória para uso imediato e agora também são persistidos no SQLite com payload redigido.
- A leitura traduzida grava WAVs gerados no cache local quando `documents.cache_audio` está habilitado, registra os metadados no SQLite e consulta o cache antes de chamar TTS novamente.
- `visionclip-infer` agora expoe `AiProvider`, `ProviderRouter`, capabilities e requests tipados para chat, visão, embeddings e tradução de documento; `OllamaBackend` implementa essa trait sem alterar o comportamento local atual.
- O daemon usa o `ProviderRouter` nos fluxos de documentos para `ask_document`, `summarize_document`, embeddings de ingestão/pergunta e tradução/leitura incremental; tudo segue sensível/local-first e roteia para Ollama local.
- O fluxo principal de captura/OCR também passa pelo `ProviderRouter`: OCR dedicado usa `AiTask::Ocr`, raciocínio sobre texto OCR usa `AiTask::Chat` e fallback visual usa `AiTask::Vision`.
- Busca enriquecida, OCR da busca renderizada e respostas do REPL também passam pelo `ProviderRouter`, preservando os prompts especializados de AI Overview e REPL.
- `[providers]` define a política inicial de roteamento: `local_first` por padrão, `sensitive_data_mode = "local_only"`, Ollama habilitado e cloud desabilitada.

## Próximos passos

1. Adicionar confirmação real via IPC/UI para tools que retornam `RequireConfirmation`.
2. Adicionar stubs cloud desabilitados por padrão sobre a política `[providers]`, mantendo `OllamaBackend` como provider local padrão.
3. Extrair DesktopController para apps/URLs, volume, brilho e VPN com command runner mockável.
4. Conectar voz e CLI ao `AgentOrchestrator` para substituir roteamento local duplicado.
5. Expor controles de cache/progresso para a UI futura de leitura.
