# Próximos Passos do Backend REPL

Este documento organiza as próximas entregas técnicas do backend do Coddy REPL, priorizando TDD, baixo acoplamento e a futura separação do Coddy para um repositório próprio.

## Estado Atual

O backend já possui:

- domínio REPL em `crates/coddy-core`;
- contratos REPL iniciais em `crates/coddy-ipc`;
- cliente de transporte inicial em `crates/coddy-client`;
- configuração mínima do CLI em `CoddyRuntimeConfig`;
- configuração neutra de microfone em `visionclip_voice_input::VoiceInputConfig`;
- protocolo direto `CoddyWireRequest`/`CoddyWireResult` aceito pelo daemon;
- comandos estruturados em `ReplCommand`;
- eventos de sessão em `ReplEvent`;
- reducer de sessão em `ReplSession::apply_event`;
- histórico incremental com `ReplEventLog`;
- publicação ao vivo com `ReplEventBroker`;
- snapshot via `coddy session snapshot`;
- polling via `coddy session events --after`;
- stream local via `coddy session watch --after`;
- testes de contrato bincode para os requests REPL;
- OpenAPI proposta para futura bridge HTTP/Tauri.

## Direção Arquitetural

O Coddy deve ser tratado como produto separado que consome serviços do VisionClip. A fronteira entre eles deve ser um protocolo estável, não chamadas diretas para detalhes internos.

Modelo alvo:

```text
Coddy UI/CLI -> coddy-client -> coddy-ipc -> VisionClip daemon -> serviços Linux/AI
```

Regra prática: o Coddy pode depender de `coddy-core`, `coddy-ipc` e `coddy-client`; ele não deve depender diretamente de `visionclip-common` quando for movido para outro repositório.

## Fase 1: Consolidar Stream de Eventos

Objetivo: tornar o stream a forma principal de sincronização da UI.

Entregas:

- manter `coddy session snapshot` como bootstrap;
- usar `CoddyClient::event_watcher(last_sequence)` ou `coddy session watch --after <last_sequence>` para atualizações em tempo real;
- manter `coddy session events --after` como fallback de reconexão;
- manter política de reconnect no cliente;
- documentar formato NDJSON/SSE para bridges futuras.

Critérios de aceite:

- cliente consegue abrir stream, receber replay e eventos ao vivo;
- queda de conexão não perde estado se cliente reconectar com `last_sequence`;
- reducer TypeScript usa o mesmo envelope de evento do backend;
- testes cobrem replay, evento ao vivo e reconnect por sequence.

## Fase 2: Completar `coddy-ipc`

Objetivo: remover a dependência futura do Coddy em `visionclip-common`.

Estado atual:

- `crates/coddy-ipc` já existe no workspace;
- `CODDY_PROTOCOL_VERSION` já foi definido;
- `CoddyEnvelope<T>` já existe para versionamento futuro;
- `read_frame` e `write_frame` já centralizam o framing bincode genérico;
- `CoddyRequest` e `CoddyResult` já existem como fronteira pública do cliente Coddy;
- `CoddyWireRequest` e `CoddyWireResult` já isolam o protocolo direto com magic `CDDY`;
- `decode_wire_request_payload` e `decode_wire_result_payload` centralizam detecção de payload direto vs. fallback legado;
- os jobs `ReplCommandJob`, `ReplSessionSnapshotJob`, `ReplEventsJob` e `ReplEventStreamJob` já foram movidos para `coddy-ipc`;
- `visionclip-common` ainda mantém os tipos legados para compatibilidade com clientes VisionClip.

Entregas:

- manter erro estruturado para incompatibilidade de protocolo;
- manter documentação em `docs/repl/coddy-wire-contract.md` sincronizada com `coddy-ipc`;
- remover o protocolo legado do caminho REPL quando não houver mais clientes antigos dependendo dele.

Tipos candidatos:

- `CoddyProtocolError`.

Critérios de aceite:

- `apps/coddy` não importa `visionclip-common`;
- `coddy-ipc` não importa crates VisionClip;
- testes de serialização permanecem estáveis;
- daemon rejeita versão incompatível com mensagem clara.

## Fase 3: Criar `coddy-client`

Objetivo: esconder detalhes de transporte do CLI e da UI.

Estado atual:

- `crates/coddy-client` já existe no workspace;
- `apps/coddy` já usa `CoddyClient` para `send_command`, `snapshot`, `events_after` e `event_stream`;
- comandos internos como `stop_speaking` e `stop_active_run` já têm métodos dedicados no client;
- o client já expõe `CoddyResult` para consumidores;
- `CoddyClientOptions` já define timeouts de conexão e request;
- `ReplEventWatcher` já reconecta após EOF usando backoff e o último `last_sequence`;
- se o daemon/socket cair durante a reconexão, o watcher continua tentando até o stream voltar;
- o client fala `CoddyWireRequest`/`CoddyWireResult` diretamente e não depende de `visionclip-common`.

Entregas:

- interface preparada para Tauri command e HTTP local;
- logs com `request_id`.

Critérios de aceite:

- CLI não abre socket manualmente;
- UI TypeScript pode usar uma bridge fina sem duplicar protocolo;
- testes usam transporte fake para simular daemon.

## Fase 4: Persistência de Sessão

Objetivo: preservar histórico recente e melhorar reconexão da UI.

Entregas:

- SQLite para sessões e eventos recentes;
- retenção configurável por número de eventos ou tempo;
- limpeza de dados sensíveis;
- modo privado sem persistência;
- migrações versionadas.

Critérios de aceite:

- reiniciar daemon não zera imediatamente a sessão ativa recente;
- dados sensíveis não são salvos por padrão;
- testes cobrem replay a partir de banco local.

## Fase 5: Streaming de Tokens do LLM

Objetivo: reduzir latência percebida no REPL.

Entregas:

- emitir `TokenDelta` conforme o modelo local gera resposta;
- acumular resposta final em `MessageAppended`;
- permitir cancelamento de run ativo;
- iniciar TTS de forma segura apenas quando houver texto suficiente;
- evitar sobreposição de voz com o gate atual.

Critérios de aceite:

- UI renderiza resposta parcial;
- `StopActiveRun` interrompe geração;
- `StopSpeaking` não corrompe a sessão;
- testes cobrem ordem `RunStarted -> TokenDelta* -> MessageAppended -> RunCompleted`.

## Fase 6: Ações Nativas e Router

Objetivo: separar perguntas gerais, abertura de apps/sites, leitura de tela e ações sensíveis.

Entregas:

- action registry compartilhado com schemas;
- resolver de aplicativos Linux exposto como ação;
- resolver de sites conhecidos sem confundir com busca;
- política de risco por ação;
- confirmação para ações médias/altas;
- eventos `ToolStarted` e `ToolCompleted` para todas as ações.

Critérios de aceite:

- "terminal", "vscode", "youtube" e "pesquise JavaScript" seguem intents diferentes;
- LLM nunca executa shell diretamente;
- testes cobrem aliases, ambiguidade e recusas.

## Fase 7: Integração UI TypeScript

Objetivo: conectar o backend ao terminal flutuante e ao desktop app.

Entregas:

- boot com snapshot;
- stream com `watch`;
- reducer TypeScript espelhando `ReplSession::apply_event`;
- estados visuais para listening, thinking, streaming, speaking e error;
- botão de microfone;
- opção de TTS no REPL;
- painel de logs/eventos para debug.

Critérios de aceite:

- UI não chama comandos VisionClip diretamente;
- toda atualização visual vem de eventos;
- reconexão não duplica mensagens;
- testes simulam 1000 eventos sem travar input.

## Fase 8: Separação de Repositório

Objetivo: mover Coddy sem quebrar o VisionClip.

Sequência recomendada:

1. Extrair `coddy-ipc` no monorepo atual.
2. Criar `coddy-client`.
3. Remover imports de `visionclip-common` em `apps/coddy`.
4. Publicar ou referenciar `coddy-core`, `coddy-ipc` e `coddy-client` por Git/path.
5. Mover `apps/coddy` e UI para novo repositório.
6. Manter CI de contrato nos dois lados.
7. Versionar releases compatíveis entre Coddy e VisionClip daemon.

Critérios de aceite:

- Coddy compila fora do monorepo;
- VisionClip daemon compila com crates Coddy como dependências externas;
- comandos `snapshot`, `events`, `watch`, `ask` e `voice` funcionam entre repos;
- documentação indica versões compatíveis.

## Ordem Recomendada de Implementação

1. Completar `coddy-ipc` com request/result e erro de protocolo.
2. Criar `coddy-client`.
3. Migrar CLI para `coddy-client`.
4. Conectar UI ao snapshot + watch.
5. Persistir sessão em SQLite.
6. Emitir `TokenDelta`.
7. Expandir action registry e router.
8. Separar repositório.

Essa ordem reduz risco porque estabiliza o contrato antes de mover arquivos fisicamente.
