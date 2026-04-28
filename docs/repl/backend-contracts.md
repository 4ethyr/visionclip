# Contratos do Backend Coddy REPL

Este documento descreve o backend implementado para o Coddy REPL na branch atual. O objetivo é dar ao frontend TypeScript um contrato estável para sessão, comandos, eventos e sincronização incremental.

## Estado Atual

O Coddy ainda não expõe um servidor HTTP. O transporte real é IPC local via Unix socket usando mensagens bincode length-prefixed em `visionclip_common::ipc`.

Componentes implementados:

- `apps/coddy`: CLI de alto nível do REPL.
- `apps/visionclip-daemon`: daemon local que executa ações, busca, TTS e estado REPL.
- `crates/coddy-core`: domínio puro de sessão, comandos, eventos, políticas e busca.
- `crates/common`: contratos IPC compartilhados entre CLI e daemon.
- `crates/voice-input`: captura e transcrição de voz local.

## Comandos CLI

### Pergunta textual

```bash
coddy ask "Quem foi Rousseau?"
```

Fluxo:

1. CLI envia `VisionRequest::ReplCommand(ReplCommand::Ask)`.
2. Daemon registra `RunStarted`, `MessageAppended`, `IntentDetected`.
3. Daemon executa busca atual.
4. Daemon registra `SearchStarted`, `SearchContextExtracted`, `RunCompleted`.
5. Resultado final retorna como `JobResult::BrowserQuery`.

### Voz com overlay

```bash
coddy --speak voice --overlay
```

Fluxo:

1. CLI cria lock de atalho em `XDG_RUNTIME_DIR/visionclip/coddy-voice.lock`.
2. CLI abre overlay GTK quando compilado com `gtk-overlay`.
3. CLI captura/transcreve audio local via `visionclip-voice-input`.
4. CLI envia `ReplCommand::VoiceTurn { transcript_override: Some(text) }`.
5. Daemon classifica o transcript:
   - app conhecido ou comando de abertura: `OpenApplication`.
   - site conhecido: `OpenWebsite`.
   - pergunta geral: `SearchDocs`.
6. Daemon executa ação e registra eventos.

### Snapshot da sessão

```bash
coddy session snapshot
```

Retorna JSON de `ReplSessionSnapshot`:

```json
{
  "session": {
    "id": "uuid",
    "mode": "FloatingTerminal",
    "status": "Idle",
    "policy": "UnknownAssessment",
    "selected_model": {
      "provider": "ollama",
      "name": "gemma4:e2b"
    },
    "voice": {
      "enabled": true,
      "speaking": false,
      "muted": false
    },
    "screen_context": null,
    "workspace_context": [],
    "messages": [],
    "active_run": null
  },
  "last_sequence": 1
}
```

### Eventos incrementais

```bash
coddy session events --after 10
```

Retorna eventos com `sequence > 10` e o `last_sequence` atual:

```json
{
  "last_sequence": 13,
  "events": [
    {
      "sequence": 11,
      "session_id": "uuid",
      "run_id": "uuid",
      "captured_at_unix_ms": 1775000000000,
      "event": {
        "SearchStarted": {
          "query": "Quem foi Rousseau?",
          "provider": "google"
        }
      }
    }
  ]
}
```

## Contratos IPC

### Requests

| Request | Uso | Resposta esperada |
| --- | --- | --- |
| `VisionRequest::ReplCommand` | Executa `Ask`, `VoiceTurn`, `StopSpeaking`, `StopActiveRun` e comandos futuros. | `BrowserQuery`, `ActionStatus` ou `Error`. |
| `VisionRequest::ReplSessionSnapshot` | Retorna estado reduzido da sessão. | `JobResult::ReplSessionSnapshot`. |
| `VisionRequest::ReplEvents` | Retorna eventos incrementais depois de uma sequence. | `JobResult::ReplEvents`. |
| `VisionRequest::OpenApplication` | Abre app Linux via resolvedor `.desktop`. | `ActionStatus`. |
| `VisionRequest::OpenUrl` | Abre URL HTTP/HTTPS no navegador. | `ActionStatus`. |
| `VisionRequest::VoiceSearch` | Executa busca por voz legada. | `BrowserQuery`. |
| `VisionRequest::Capture` | Executa captura/OCR/LLM. | `ClipboardText` ou `BrowserQuery`. |

### Resultados

| Resultado | Uso |
| --- | --- |
| `ClipboardText` | Texto final de OCR, explicação, tradução ou extração. |
| `BrowserQuery` | Busca aberta no navegador e resumo inicial copiado. |
| `ActionStatus` | Status de abertura de app/site ou comando interno. |
| `Error` | Erro estruturado com `code` e `message`. |
| `ReplSessionSnapshot` | Snapshot reduzido da sessão Coddy. |
| `ReplEvents` | Lote incremental de eventos REPL. |

## Modelo de Evento

Eventos são armazenados em `ReplEventLog` como `ReplEventEnvelope`:

```rust
pub struct ReplEventEnvelope {
    pub sequence: u64,
    pub session_id: Uuid,
    pub run_id: Option<Uuid>,
    pub captured_at_unix_ms: u64,
    pub event: ReplEvent,
}
```

Regras:

- `sequence` é monotônica por sessão e começa em `1`.
- `events_after(n)` retorna apenas eventos com `sequence > n`.
- `last_sequence` permite ao frontend detectar se está sincronizado.
- O log atual é em memória e reinicia com o daemon.

## Reducer de Sessão

`ReplSession::apply_event` reduz eventos para estado de UI:

| Evento | Efeito |
| --- | --- |
| `SessionStarted` | Define `id` e `Idle`. |
| `RunStarted` | Define `active_run` e `Thinking`. |
| `OverlayShown` | Atualiza `mode`. |
| `VoiceListeningStarted` | `Listening`. |
| `VoiceTranscriptPartial` | `Transcribing`. |
| `VoiceTranscriptFinal` | `Thinking`. |
| `SearchStarted` | `Thinking`. |
| `SearchContextExtracted` | `BuildingContext`. |
| `TokenDelta` | `Streaming`. |
| `MessageAppended` | Adiciona mensagem ao histórico. |
| `TtsStarted` | Marca `voice.speaking = true` e `Speaking`. |
| `TtsCompleted` | Marca `voice.speaking = false`. |
| `RunCompleted` | Limpa `active_run` e volta para `Idle` se não houver TTS ativo. |
| `Error` | `Error`. |

## Integração Recomendada Para o Frontend

Fluxo inicial:

1. Chamar `coddy session snapshot` ou endpoint equivalente.
2. Renderizar `snapshot.session`.
3. Guardar `snapshot.last_sequence`.
4. Fazer polling com `coddy session events --after <last_sequence>`.
5. Aplicar eventos no reducer TypeScript na ordem de `sequence`.
6. Atualizar `last_sequence` com a resposta.

Esse desenho funciona tanto para:

- CLI/floating terminal via Unix socket.
- Tauri commands.
- Bridge HTTP local documentada em OpenAPI.

## Limitações Conhecidas

- O event log ainda é em memória; não há persistência entre reinícios do daemon.
- Ainda não existe stream longo ou WebSocket; o contrato atual é polling incremental.
- `RunStarted` usa `request_id` como `run_id` para o MVP.
- `TokenDelta` ainda não é emitido pelo LLM; o daemon retorna respostas finais.
- Snapshot e eventos ainda não expõem autenticação porque o transporte real é Unix socket local.
- A especificação OpenAPI documenta a futura ponte HTTP/Tauri, não um servidor HTTP ativo hoje.

## Próximas Fatias

1. Criar `ReplEventBroker` com `tokio::sync::broadcast` para streaming real.
2. Expor stream para Tauri channels no app desktop.
3. Persistir eventos/sessões recentes em SQLite.
4. Emitir `TokenDelta` quando o backend LLM suportar streaming.
5. Adicionar comandos `CaptureAndExplain`, `OpenUi` e `SelectModel` ao daemon.
6. Conectar frontend TypeScript a `snapshot` + `events --after`.
