# Contratos do Backend Coddy REPL

Este documento descreve o backend implementado para o Coddy REPL na branch atual. O objetivo é dar ao frontend TypeScript um contrato estável para sessão, comandos, eventos e sincronização incremental.

## Estado Atual

O Coddy ainda não expõe um servidor HTTP. O transporte real é IPC local via Unix socket usando mensagens bincode length-prefixed de `coddy-ipc`.

O daemon mantém compatibilidade com o protocolo legado `VisionRequest`/`JobResult`, mas o CLI Coddy fala o protocolo direto `CoddyWireRequest`/`CoddyWireResult`.

Componentes implementados:

- `apps/coddy`: CLI de alto nível do REPL.
- `apps/visionclip-daemon`: daemon local que executa ações, busca, TTS e estado REPL.
- `crates/coddy-core`: domínio puro de sessão, comandos, eventos, políticas e busca.
- `crates/coddy-ipc`: protocolo direto do Coddy, versionamento e framing bincode.
- `crates/coddy-client`: cliente Unix socket usado pelo CLI e preparado para UI.
- `crates/common`: contratos legados VisionClip e configuração do daemon.
- `crates/voice-input`: captura e transcrição de voz local com configuração neutra `VoiceInputConfig`.

## Comandos CLI

### Pergunta textual

```bash
coddy ask "Quem foi Rousseau?"
```

Fluxo:

1. CLI envia `CoddyWireRequest::new(CoddyRequest::Command(ReplCommand::Ask))`.
2. Daemon registra `RunStarted`, `MessageAppended`, `IntentDetected`.
3. Daemon executa busca atual.
4. Daemon registra `SearchStarted`, `SearchContextExtracted`, `RunCompleted`.
5. Resultado final retorna como `CoddyResult::BrowserQuery`.

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

### Stream de eventos

```bash
coddy session watch --after 10
```

Mantém a conexão Unix socket aberta e envia frames `CoddyResult::ReplEvents` pelo protocolo direto. O CLI imprime cada frame em JSON por linha para consumo por UI/bridge.

Esse modo é o contrato preferencial para a UI flutuante/desktop: a tela carrega `coddy session snapshot` uma vez, aplica eventos antigos com `events --after <last_sequence>` quando necessário e depois mantém `watch --after <last_sequence>` para atualizações em tempo real.

## Contratos IPC

Os contratos específicos do Coddy ficam em `crates/coddy-ipc`, com `CODDY_PROTOCOL_VERSION = 1`, `CODDY_PROTOCOL_MAGIC = "CDDY"`, `CoddyWireRequest`, `CoddyRequest`, `CoddyWireResult` e `CoddyResult`.

O CLI usa `crates/coddy-client` para acessar o daemon. Esse client centraliza socket, snapshot, eventos incrementais, stream e comandos, retorna `CoddyResult` para consumidores e evita que `apps/coddy` conheça detalhes de transporte ou tipos legados do VisionClip.

### Requests

| Request | Uso | Resposta esperada |
| --- | --- | --- |
| `CoddyRequest::Command` | Executa `Ask`, `VoiceTurn`, `StopSpeaking`, `StopActiveRun` e comandos futuros. | `BrowserQuery`, `ActionStatus` ou `Error`. |
| `CoddyRequest::SessionSnapshot` | Retorna estado reduzido da sessão. | `CoddyResult::ReplSessionSnapshot`. |
| `CoddyRequest::Events` | Retorna eventos incrementais depois de uma sequence. | `CoddyResult::ReplEvents`. |
| `CoddyRequest::EventStream` | Mantém uma conexão persistente e publica eventos depois de uma sequence. | Múltiplos `CoddyResult::ReplEvents`, um por frame emitido. |

Requests legados ainda aceitos pelo daemon:

| Request legado | Uso | Resposta esperada |
| --- | --- | --- |
| `VisionRequest::ReplCommand` | Compatibilidade com clientes VisionClip antigos. | `JobResult::BrowserQuery`, `ActionStatus` ou `Error`. |
| `VisionRequest::ReplSessionSnapshot` | Compatibilidade para snapshot de sessão. | `JobResult::ReplSessionSnapshot`. |
| `VisionRequest::ReplEvents` | Compatibilidade para polling incremental. | `JobResult::ReplEvents`. |
| `VisionRequest::ReplEventStream` | Compatibilidade para stream persistente legado. | Múltiplos `JobResult::ReplEvents`. |
| `VisionRequest::OpenApplication` | Abre app Linux via resolvedor `.desktop`. | `ActionStatus`. |
| `VisionRequest::OpenUrl` | Abre URL HTTP/HTTPS no navegador. | `ActionStatus`. |
| `VisionRequest::VoiceSearch` | Executa busca por voz legada. | `BrowserQuery`. |
| `VisionRequest::Capture` | Executa captura/OCR/LLM. | `ClipboardText` ou `BrowserQuery`. |

### Resultados Coddy

| Resultado | Uso |
| --- | --- |
| `CoddyResult::Text` | Texto final de OCR, explicação, tradução ou extração. |
| `CoddyResult::BrowserQuery` | Busca aberta no navegador e resumo inicial copiado. |
| `CoddyResult::ActionStatus` | Status de abertura de app/site ou comando interno. |
| `CoddyResult::Error` | Erro estruturado com `code` e `message`. |
| `CoddyResult::ReplSessionSnapshot` | Snapshot reduzido da sessão Coddy. |
| `CoddyResult::ReplEvents` | Lote incremental de eventos REPL. |

## Modelo de Evento

Eventos são armazenados em `ReplEventLog` e publicados por `ReplEventBroker` como `ReplEventEnvelope`:

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
- `ReplEventBroker` mantém replay histórico e publica novos envelopes via `tokio::sync::broadcast`.
- `ReplEventStream` usa o mesmo envelope de `ReplEvents`, então clientes podem alternar entre polling e stream sem reducer diferente.
- Clientes devem rejeitar frames de stream cujo `event.sequence` não avance em relação ao cursor local ou cujo `last_sequence` não corresponda ao evento enviado.

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
4. Abrir `CoddyClient::event_watcher(last_sequence)` ou `coddy session watch --after <last_sequence>` para receber eventos em tempo real.
5. Aplicar eventos no reducer TypeScript na ordem de `sequence`.
6. Atualizar `last_sequence` a cada frame recebido.

Fallback sem stream:

- Usar `coddy session events --after <last_sequence>` em polling curto.
- Reabrir `watch` com o último `last_sequence` conhecido se a conexão cair, ou delegar isso ao `ReplEventWatcher`.

Esse desenho funciona tanto para:

- CLI/floating terminal via Unix socket.
- Tauri commands.
- Bridge HTTP local documentada em OpenAPI.

## Limitações Conhecidas

- O event log ainda é em memória; não há persistência entre reinícios do daemon.
- O stream atual é Unix socket local com frames bincode; WebSocket/SSE fica para a bridge HTTP/Tauri.
- `RunStarted` usa `request_id` como `run_id` para o MVP.
- `TokenDelta` ainda não é emitido pelo LLM; o daemon retorna respostas finais.
- Snapshot e eventos ainda não expõem autenticação porque o transporte real é Unix socket local.
- A especificação OpenAPI documenta a futura ponte HTTP/Tauri, não um servidor HTTP ativo hoje.

## Próximas Fatias

1. Expor `ReplEventStream` para Tauri channels no app desktop.
2. Criar bridge HTTP/SSE ou WebSocket local para consumidores externos quando necessário.
3. Persistir eventos/sessões recentes em SQLite.
4. Emitir `TokenDelta` quando o backend LLM suportar streaming.
5. Adicionar comandos `CaptureAndExplain`, `OpenUi` e `SelectModel` ao daemon.
6. Conectar frontend TypeScript a `snapshot` + `watch`.
