# Coddy Client

`crates/coddy-client` é o adaptador de transporte do Coddy. Ele centraliza como CLI e futura UI falam com o daemon local.

## Objetivo

Evitar que `apps/coddy` e a UI TypeScript conheçam detalhes de:

- caminho do Unix socket;
- framing bincode;
- `VisionRequest`;
- `JobResult`;
- abertura e ciclo de vida de conexões;
- stream persistente de eventos.

No futuro, essa crate deve ser movida para o repositório do Coddy junto com `coddy-core` e `coddy-ipc`.

## Estado Atual

O client já expõe:

```rust
CoddyClient::new(socket_path)
CoddyClient::with_options(socket_path, options)
CoddyClient::send_command(command, speak)
CoddyClient::stop_speaking()
CoddyClient::stop_active_run()
CoddyClient::snapshot()
CoddyClient::events_after(after_sequence)
CoddyClient::event_stream(after_sequence)
CoddyClient::event_watcher(after_sequence)
```

Comandos genéricos retornam `CoddyResult`, definido em `coddy-ipc`, e não `JobResult` do VisionClip.

`CoddyClientOptions` define timeouts por operação:

```rust
CoddyClientOptions {
    connect_timeout: Duration::from_secs(2),
    request_timeout: Duration::from_secs(180),
}
```

O timeout de request cobre roundtrips e abertura inicial do stream. `ReplEventStream::next()` não usa timeout por padrão porque é uma conexão persistente; política de reconnect deve ficar no consumidor ou numa camada superior.

O client valida correlação de `request_id`: respostas roundtrip e frames de stream com `request_id` diferente do request original são rejeitados.

O stream também valida monotonicidade: cada frame precisa trazer exatamente um evento com `sequence` maior que a última sequência conhecida, e `last_sequence` precisa corresponder à `sequence` desse evento. Isso protege a UI contra duplicação, regressão e perda silenciosa de cursor após bugs no daemon ou em bridges intermediárias.

Para UI/bridge, prefira `event_watcher(after_sequence)` quando quiser reconexão automática. O watcher:

- abre `event_stream(after_sequence)`;
- retorna frames individuais;
- atualiza `last_sequence` a cada frame;
- em EOF, reabre o stream com o último `last_sequence`;
- se o socket estiver indisponível durante a reconexão, continua tentando sem derrubar a sessão do consumidor;
- aplica backoff usando `reconnect_initial_delay` e `reconnect_max_delay`.

Use `event_stream(after_sequence)` diretamente apenas quando o consumidor já possui sua própria política de reconexão.

`ReplEventStream::next()` retorna frames individuais com:

```rust
pub struct ReplEventStreamFrame {
    pub request_id: Uuid,
    pub event: ReplEventEnvelope,
    pub last_sequence: u64,
}
```

## Limite Temporário

O daemon já aceita `CoddyWireRequest` e responde `CoddyWireResult` diretamente pelo socket local.

O protocolo legado `VisionRequest`/`JobResult` continua disponível no daemon apenas para compatibilidade com clientes VisionClip existentes. O `coddy-client` não depende mais de `visionclip-common` e não monta mensagens legadas.

O framing bincode vem de `coddy-ipc` por `read_frame` e `write_frame`. O envelope direto usa magic `CDDY` e `CODDY_PROTOCOL_VERSION` para evitar colisão com frames legados.

## Arquitetura Alvo

`coddy-ipc` já define:

```rust
CoddyRequest
CoddyResult
CoddyWireRequest
CoddyWireResult
CoddyEnvelope<T>
CODDY_PROTOCOL_VERSION
```

Estado atual:

- `coddy-client` não depende de `visionclip-common`;
- o daemon VisionClip implementa o servidor do protocolo `coddy-ipc`;
- `apps/coddy` usa `CoddyRuntimeConfig`, compatível com o TOML atual do VisionClip para `general.log_level` e `voice`;
- a UI poderá usar o mesmo client por Tauri command, HTTP local ou Unix socket.

O próximo passo de desacoplamento é mover `apps/coddy`, `coddy-core`, `coddy-ipc`, `coddy-client` e `visionclip-voice-input` para o repositório Coddy mantendo o socket do daemon como fronteira.

## Fluxo Recomendado Para UI

1. Criar client com o socket configurado.
2. Chamar `snapshot()`.
3. Renderizar a sessão inicial.
4. Abrir `event_watcher(snapshot.last_sequence)`.
5. Aplicar cada `ReplEventEnvelope` no reducer da UI.
6. Em queda de conexão, deixar o watcher reabrir o stream com o último `last_sequence`.

## Próximas Melhorias

- Separar trait de transporte para testes e bridges Tauri/HTTP.
- Adicionar testes de contrato executáveis entre o repositório Coddy e o daemon VisionClip.
