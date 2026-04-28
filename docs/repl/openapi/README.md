# OpenAPI/Swagger do Coddy REPL

Este diretório documenta o contrato HTTP proposto para uma futura bridge local do Coddy REPL.

## Arquivo Principal

- [`coddy-repl.openapi.yaml`](coddy-repl.openapi.yaml)

## Escopo

A aplicação atual ainda usa Unix socket com mensagens bincode. A especificação OpenAPI descreve uma camada HTTP/Tauri futura, mantendo os mesmos conceitos:

- sessão REPL;
- snapshot;
- eventos incrementais;
- comandos estruturados;
- voice turn;
- stop speaking;
- stop active run.

Essa especificação pode ser usada por:

- Swagger UI;
- geradores de cliente TypeScript;
- testes de contrato;
- documentação de API;
- ferramentas compatíveis com OpenAPI, incluindo actions que aceitem schema OpenAPI.

## Decisão Técnica

A especificação usa `openapi: 3.1.0` por compatibilidade ampla com Swagger UI e geradores de cliente. O runtime Rust continua sendo a fonte de verdade; qualquer alteração nos enums `ReplCommand`, `ReplEvent`, `ReplSession` ou `CoddyResult` deve atualizar este arquivo.

## Como Evoluir

Quando o bridge HTTP existir, ele deve:

1. Escutar somente em loopback ou Unix socket convertido por gateway local.
2. Não expor endpoints de execução shell arbitrária.
3. Validar todos os comandos contra action registry.
4. Pedir confirmação para ações de risco médio/alto.
5. Manter os nomes de `operationId` estáveis para clientes gerados.

## Endpoints Modelados

| Metodo | Endpoint | Uso |
| --- | --- | --- |
| `GET` | `/v1/repl/session` | Snapshot da sessão. |
| `GET` | `/v1/repl/events` | Eventos incrementais por `after_sequence`. |
| `GET` | `/v1/repl/events/stream` | Stream SSE proposto para eventos em tempo real. |
| `POST` | `/v1/repl/commands` | Envia comando REPL estruturado. |
| `POST` | `/v1/repl/voice-turns` | Envia transcript de voz ou pede captura ASR no cliente. |
| `POST` | `/v1/repl/speech/stop` | Para fala atual. |
| `POST` | `/v1/repl/runs/{run_id}/stop` | Cancela execução ativa. |

## Validacao Recomendada

Antes de publicar uma mudança de contrato:

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Quando houver toolchain Node no projeto:

```bash
npx @redocly/cli lint docs/repl/openapi/coddy-repl.openapi.yaml
```
