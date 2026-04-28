# AGENTS.md — VisionClip / Coddy REPL

> Guia para agentes de IA trabalharem neste repositório de forma produtiva.

## Visão Geral

Este é um **monorepo Rust workspace** com um **frontend Electron + React + TypeScript** (`apps/coddy-electron/`) e múltiplos **crates Rust** formando o backend do assistente Linux VisionClip e do Coddy REPL. O projeto está em processo de **desacoplamento**: o Coddy será extraído para repositório próprio (ver `docs/repl/coddy-decoupling-plan.md`).

```
apps/           ← binários Rust (visionclip, coddy, daemon, config)
crates/         ← libs Rust (coddy-core, coddy-ipc, coddy-client, common, infer, etc.)
apps/coddy-electron/  ← frontend TypeScript (Electron + React + Tailwind)
repl_ui/        ← mockups HTML estáticos de referência visual
docs/repl/      ← documentação de arquitetura, contratos, planos
```

## Comandos Essenciais

### Rust

| Comando | Finalidade |
|---------|-----------|
| `cargo build --workspace` | Compila todos os crates/apps |
| `cargo test --workspace`  | Executa todos os testes Rust |
| `cargo test -p coddy-core` | Testa só o domínio do Coddy |
| `cargo test -p coddy-ipc`  | Testa protocolo IPC (bincode roundtrip) |
| `cargo test -p coddy-client` | Testa cliente IPC (Unix socket simulado) |
| `cargo run -p coddy -- ask "pergunta"` | CLI REPL textual |
| `cargo run -p visionclip-daemon` | Inicia o daemon local |

### Frontend (coddy-electron)

```bash
cd apps/coddy-electron

npm install        # instalar dependências (node_modules fora do repo)
npm test           # vitest run (testes de domínio com coverage threshold 80%)
npm run test:watch # vitest em watch mode
npm run typecheck  # tsc --noEmit (verificar tipos)
npm run build      # tsc + vite build
npm run lint       # eslint src/ --ext .ts,.tsx
```

O `vitest` usa aliases `@/` → `src/` e o plugin `@vitejs/plugin-react`. O `jsdom` é usado como ambiente de teste.

### Scripts auxiliares

```bash
bash scripts/guard_no_secrets.sh   # pre-commit: bloqueia secrets em commits
bash scripts/start_local_stack.sh  # inicia daemon + serviços locais
bash scripts/stop_local_stack.sh   # para stack local
bash scripts/test_tts_flows.sh     # testa fluxos TTS
```

## Arquitetura

### Camada Rust (backend)

```
coddy-core    → Domínio puro: tipos, reducers, event log, broker, policies
                 NÃO depende de crate alguma do VisionClip ✅
coddy-ipc     → Protocolo IPC: CoddyRequest/CoddyResult, framing bincode,
                 CODDY_PROTOCOL_VERSION, envelope de compatibilidade.
                 Só depende de coddy-core ✅
coddy-client  → Cliente Unix socket que CLI e UI usam. Internamente
                 faz mapping CoddyRequest ↔ VisionRequest, mas ainda
                 importa visionclip-common ❌ (a ser resolvido)
common        → visionclip-common: tipos IPC do VisionClip + reexporta
                 tipos coddy-ipc. Depende de coddy-core e coddy-ipc ❌
```

**Status do desacoplamento**: `coddy-core` e `coddy-ipc` já estão limpos. `coddy-client` ainda depende de `visionclip-common` porque o daemon atual só entende `VisionRequest`/`JobResult`. A meta é o daemon passar a implementar `CoddyRequest`/`CoddyResult` direto, removendo a camada de mapping.

### Camada Frontend (coddy-electron)

O frontend segue **Clean Architecture com 4 camadas**:

```
domain/          ← Tipos puros, reducers, funções factory (não importa React)
  types/          espelha crates/coddy-core/src/ (event.ts ≡ event.rs, etc.)
  reducers/       sessionReducer, policyEvaluator (funções puras)
  contracts/      (ainda vazio — interfaces para infra)
application/     ← Casos de uso / orquestração (ainda vazio)
infrastructure/  ← Implementações concretas (IPC, persistência)
  ipc/            (ainda vazio — será o cliente IPC do Electron)
presentation/    ← React: componentes, hooks, views
  views/
    FloatingTerminal/   ← modo simples (ainda vazio)
    DesktopApp/         ← modo advanced (ainda vazio)
  components/     ← componentes compartilhados (ainda vazio)
  hooks/          ← hooks customizados (ainda vazio)
main/            ← Electron main process (ainda vazio)
```

### Fluxo de Dados Frontend ← Backend

```
1. Frontend chama coddy session snapshot → recebe ReplSessionSnapshot
2. Renderiza snapshot.session (estado reduzido)
3. Guarda snapshot.last_sequence
4. Abre stream (coddy session watch --after <last_sequence>)
5. Cada ReplEventEnvelope recebido é passado ao sessionReducer
6. Reducer deriva novo estado de UI deterministicamente
```

Os eventos são emitidos pelo `ReplEventBroker` no daemon via `tokio::sync::broadcast`. O frontend deve tratar eventos como **append-only log** e derivar estado via **reducer puro** — nunca mutar estado diretamente.

### Convenção dos tipos

**Rust → TypeScript mirror**: Para cada enum/variant em Rust (`crates/coddy-core/src/`), existe um tipo equivalente no frontend (`src/domain/types/`). A convenção é:

- Enums Rust viram **union types** em TS: `'Idle' | 'Listening' | ...`
- Structs viram **interfaces**: `interface ReplSession { ... }`
- Eventos viram **discriminated union**: `{ SessionStarted: { session_id: string } } | ...`
- `id: Uuid` em Rust → `id: string` em TS
- Campos `Option<T>` → `T | null`
- `Vec<T>` → `T[]`
- `streaming_text` é frontend-only (não vem do backend) — acumulado pelo `sessionReducer` a partir de eventos `TokenDelta`

**Não invente variantes** — sempre mireie o que está implementado em Rust. Se adicionar variante nova, adicione nos dois lados.

## Estado Atual do Frontend (v2 — após expansão)

### Implementado ✅

| Camada | Arquivos | Status |
|--------|----------|--------|
| Domain Types | events.ts (23 eventos, ReplSessionSnapshot), session.ts (10 status, streaming_text), policy.ts | Completo |
| Domain Reducers | sessionReducer (TokenDelta accumulation), policyEvaluator | Completo |
| Domain Contracts | ReplIpcClient interface (8 métodos) | Completo |
| Application | SessionManager, EventStreamer (reconexão), CommandSender, SettingsStore (localStorage) | Completo |
| Infrastructure IPC | ElectronReplIpcClient (invoke/on), globals.ts (Window.replApi) | Completo |
| Electron Main | main.ts, preload.ts (contextBridge), ipcBridge.ts (spawn coddy CLI + bridge) | Completo |
| Hooks | useReplClient (singleton), useSession (lifecycle + reconexão) | Completo |
| Components | MessageBubble (markdown + code blocks), StatusIndicator (10 estados), InputBar (terminal-style), CodeBlock (syntax highlight + copy), ModelSelector (dropdown), VoiceButton (browser mic stub), Sidebar (navegação DesktopApp), WorkspacePanel, ConversationPanel | Completo |
| Views | FloatingTerminal (glass, streaming, voice, model picker, window controls), DesktopApp (sidebar + tabs) | Completo |
| App Root | App.tsx (mode switching, Escape shortcut, settings persist) | Completo |

### Por fazer 🔜

| Tarefa | Motivação |
|--------|-----------|
| Testes de componente | MessageBubble, InputBar, StatusIndicator com Testing Library |
| Testes de integração IPC | Simular Electron IPC e verificar fluxo snapshot → watch → reduce |
| Testes e2e | Playwright contra coddy CLI stub |
| Integração STT real | Enviar blob do VoiceButton para faster-whisper via daemon |
| Context-aware UI | Show screen capture context, OCR results, detected kind |
| Assessment confirmation modal | Pedir confirmação quando `requiresConfirmation` |
| Code block diff view | Agentic mode — mostrar diff antes de aplicar |
| Animação de streaming | Digitação suave com fade do cursor |

## Padrões e Convenções

## Padrões e Convenções

### Rust
- `edition = "2021"`, `resolver = "2"`
- Serde via `bincode` para IPC interno, `serde_json` para snapshot/eventos expostos
- Workspace dependencies no `[workspace.dependencies]` do `Cargo.toml` raiz
- Error handling: `anyhow` para aplicação, `thiserror` para bibliotecas
- Tokio runtime completo (`features = ["full"]`)
- Nomes de módulos em snake_case, exports via `pub use` no `lib.rs`

### TypeScript
- Path alias `@/` → `src/` (configurado em `vitest.config.ts` e `tsconfig.json`)
- Tipos estritos (`strict: true` no tsconfig)
- Reducers SEMPRE puros — retornam novo objeto, nunca mutam input
- Event tag usada como discriminante: `const tag = Object.keys(event)[0]`
- Imports de domínio sempre via barrels (`@/domain`)

### Testes
- **Rust**: testes de bincode roundtrip em `coddy-ipc/src/lib.rs`, testes de event log/broker em `coddy-core/src/event_log.rs` e `event_broker.rs`, testes de Unix socket simulado em `coddy-client/src/lib.rs`
- **TypeScript**: vitest + jsdom, cobertura mínima de 80% para `domain/` e `application/`, testes organizados em `__tests__/domain/`, `__tests__/presentation/`, `__tests__/infrastructure/`, `__tests__/e2e/`
- Teste de contrato: cada evento do reducer tem seu próprio `it(...)`, verifica transições de `SessionStatus` e imutabilidade

## Gotchas e Não-Óbvios

1. **coddy-client faz mapping bidirecional**: `map_coddy_request()` converte `CoddyRequest` → `VisionRequest`, `map_job_result()` converte `JobResult` → `CoddyResult`. O daemon ainda só fala `VisionRequest`/`JobResult`. Isso é um **acoplamento temporário** que será removido quando o daemon implementar `CoddyRequest` direto.

2. **Event log é em memória**: `ReplEventLog` e `ReplEventBroker` não persistem entre reinícios do daemon. O `last_sequence` reseta a cada restart. O frontend precisa tratar reconexão e refetch do snapshot.

3. **TokenDelta ainda não é emitido** pelo LLM — o daemon hoje retorna respostas completas, não streaming de tokens. O tipo existe no contrato mas não chega ao frontend ainda.

4. **Framing bincode**: As mensagens IPC são length-prefixed (4 bytes u32 big-endian + payload bincode). `read_frame`/`write_frame` em `coddy-ipc` encapsulam isso. Não invente outro framing.

5. **Mockups são só referência visual**: Os arquivos em `repl_ui/` são HTML estático. Eles definem o visual esperado, mas não têm lógica — toda lógica deve vir do domain layer TypeScript.

6. **`npm install` fora do repo**: `node_modules` e `dist` estão no `.gitignore`. Sempre rode `npm install` ao clonar.

7. **Electron main vs renderer**: O `main/` é o processo principal (Node.js), `presentation/` é o renderer (Chromium). A comunicação entre eles passa pelo `infrastructure/ipc/` — que ainda está vazio.

8. **Não há servidor HTTP ativo**: A spec OpenAPI em `docs/repl/openapi/` documenta uma ponte futura. Hoje o transporte real é exclusivamente Unix socket local com bincode.

## O Que Falta Construir (Frontend)

Prioridade para as próximas fatias:

1. **`infrastructure/ipc/`** — Cliente IPC que fala com o daemon via Unix socket (ou Tauri command/HTTP). Consome `snapshot` + `watch`.
2. **`application/`** — Casos de uso: iniciar sessão, enviar comando, gerenciar voz, capturar tela.
3. **`presentation/views/FloatingTerminal/`** — Modo simples: janela flutuante com input, streaming de resposta, seletor de modelo e botão de microfone.
4. **`presentation/components/`** — Componentes reutilizáveis: MessageBubble, ModelSelector, MicrophoneButton, TokenStream.
5. **`presentation/hooks/`** — useSession, useEventStream, useVoiceInput, usePolicy.
6. **`main/`** — Electron main process: criar janelas, gerenciar atalhos, conectar ao daemon.

## Referências Úteis

| Documento | Conteúdo |
|-----------|----------|
| `ARCHITECTURE.md` | Diagrama e explicação dos módulos (pt-BR) |
| `docs/repl/architecture.md` | Arquitetura detalhada do Coddy REPL |
| `docs/repl/backend-contracts.md` | Contratos exatos do backend implementado |
| `docs/repl/coddy-decoupling-plan.md` | Plano de separação Coddy ↔ VisionClip |
| `docs/repl/ui-ux-spec.md` | Especificação de UI/UX |
| `SPEC_V1.md` | Especificação V1 do VisionClip |
| `CONTRIBUTING.md` | Guia de contribuição |
| `TESTING.md` | Estratégia de testes |

## Stack

- **Backend:** Rust 2021, Tokio, Serde + bincode, Unix domain sockets
- **Frontend:** Electron 33, React 19, TypeScript 5.7, Tailwind 3.4, Vite 6, Vitest 2.1
- **Infra:** Ollama (LLM local), Piper HTTP (TTS), faster-whisper (STT)
- **Target:** Linux desktop (GNOME/GTK, Wayland/X11, D-Bus)