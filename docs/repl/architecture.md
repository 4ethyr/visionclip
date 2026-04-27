# Arquitetura do Coddy REPL

## Visão Geral

O **Coddy** deve ser uma camada de interação persistente entre usuário, tela, voz, modelos locais e ações do sistema. Ele não substitui o daemon atual do VisionClip; ele o orquestra.

Arquitetura proposta:

```mermaid
flowchart TD
  User[Usuario] --> UI[Coddy UI TypeScript + Tailwind]
  UI --> Session[ReplSession Store]
  UI --> VoiceMic[Mic UI / Push-to-talk]
  UI --> CaptureUI[Screen Context Controls]

  VoiceMic --> NativeVoice[Voice Runtime Rust]
  CaptureUI --> NativeCapture[Capture Runtime Rust]
  Session --> IPC[Tauri Commands / Unix Socket Bridge]

  IPC --> Daemon[visionclip-daemon]
  Daemon --> Router[Intent Router]
  Router --> Actions[Action Registry]
  Router --> Vision[Screen Understanding]
  Router --> Search[Web Search / Rendered AI Overview]
  Router --> LLM[Ollama / Local LLM]
  Router --> RAG[Local RAG]
  Router --> TTS[Piper TTS Queue]

  Actions --> Linux[Linux Desktop Integrations]
  Vision --> OCR[OCR / GLM / Gemma / Future OCR Provider]
  RAG --> Store[(SQLite/Tantivy/Vector Store)]

  Daemon --> Events[Structured Events Stream]
  Events --> Session
  Session --> UI
```

## Componentes Principais

### `apps/coddy`

Novo app Tauri recomendado para o modo desktop app e para o binário `coddy`.

Responsabilidades:

- Criar janelas desktop: `main`, `floating-terminal`, `settings`, `model-manager`.
- Registrar atalhos quando o app estiver ativo.
- Conectar UI TypeScript com o daemon.
- Encapsular permissões nativas.
- Controlar transparência, blur, always-on-top e focus behavior.

### Superfície de Comandos `coddy`

O binário `coddy` deve ser a entrada de alto nível para REPL, voz e assistência técnica. Ele não deve substituir `visionclip`, que continua sendo o cliente direto de captura/ações. A diferença esperada:

- `visionclip`: comando operacional direto para screenshot, OCR, tradução, explicação, busca e abertura de apps/sites.
- `coddy`: sessão conversacional, histórico, contexto, políticas, UI e fluxos agentic.

Comandos propostos:

```bash
coddy                         # abre modo desktop app ou última UI usada
coddy repl                    # abre terminal flutuante
coddy ask "explique esse erro"
coddy screen explain          # captura tela e explica
coddy screen code             # captura tela e analisa código visível
coddy screen choice           # captura tela e extrai questão/opções
coddy voice                   # push-to-talk com overlay
coddy models                  # lista modelos locais e status
coddy doctor                  # diagnóstico específico do REPL/UI
coddy settings                # abre configurações
```

Todos esses comandos devem gerar `ReplCommand` estruturado e passar pelo daemon ou pelo core Rust, nunca por shell arbitrário.

### `crates/coddy-core`

Crate Rust com domínio independente de UI.

Responsabilidades:

- Estado de sessão.
- Tipos de eventos.
- Tipos de mensagens.
- Políticas de assessment.
- Contratos de intents do REPL.
- Builders de contexto.
- Serialização/validação.

Proposta de módulos:

```text
crates/coddy-core/src/
  lib.rs
  session.rs
  event.rs
  command.rs
  assessment.rs
  context.rs
  policy.rs
  prompt.rs
  telemetry.rs
```

### `apps/coddy/src`

Frontend TypeScript + Tailwind.

Responsabilidades:

- Renderizar modo simples e advanced.
- Manter store local de UI.
- Exibir streaming de tokens/eventos.
- Suportar entrada por texto e voz.
- Gerenciar seleção de modelo.
- Exibir contexto da tela, arquivos e histórico.
- Enviar comandos estruturados ao backend.

Sugestão de stack:

- Vite
- React
- TypeScript
- TailwindCSS
- xterm.js
- Monaco Editor
- Zustand ou reducer próprio para UI state
- Vitest + Testing Library
- Playwright

## Modos de UI

### Modo Simples: Floating Terminal

Baseado em:

- `repl_ui/floating_terminal_coding_interaction/code.html`
- `repl_ui/floating_terminal_model_selection/code.html`

Características:

- Janela flutuante.
- Fundo transparente configurável.
- `backdrop-filter` com blur.
- Input terminal-like.
- Ícone de microfone.
- Seletor rápido de modelo.
- Resposta streaming estilo agent CLI.
- Baixa fricção: abrir, perguntar, responder, fechar.

Casos de uso:

- Dúvidas rápidas.
- Perguntas sobre tela.
- Explicar erro visível.
- Pedir dica durante estudo.
- Selecionar modelo local/cloud.

### Modo Advanced: Desktop App

Baseado em:

- `repl_ui/repl_main_view/code.html`
- `repl_ui/agentic_execution_mode/code.html`
- `repl_ui/context_workspace/code.html`
- `repl_ui/local_model_manager/code.html`
- `repl_ui/configuration_settings_modal/code.html`

Características:

- Sidebar fixa.
- Painel de conversa.
- Workspace de contexto.
- Gerenciador de modelos.
- Execução agentic com plano e autorização.
- Console de logs.
- Configuração de provedores, modelos, voz, privacidade e políticas.

Casos de uso:

- Sessões longas.
- Análise de projeto.
- Depuração com vários arquivos.
- RAG local.
- Monitoramento de modelos.
- Assistência em coding practice.

## Modelo de Sessão

```rust
pub struct ReplSession {
    pub id: SessionId,
    pub mode: ReplMode,
    pub status: SessionStatus,
    pub policy: AssistancePolicy,
    pub selected_model: ModelRef,
    pub voice: VoiceState,
    pub screen_context: Option<ScreenContext>,
    pub workspace_context: Vec<ContextItem>,
    pub messages: Vec<ReplMessage>,
    pub active_run: Option<RunId>,
}
```

```rust
pub enum ReplMode {
    FloatingTerminal,
    DesktopApp,
}

pub enum SessionStatus {
    Idle,
    Listening,
    Transcribing,
    CapturingScreen,
    BuildingContext,
    Thinking,
    Streaming,
    Speaking,
    AwaitingConfirmation,
    Error,
}
```

## Eventos

O daemon deve emitir eventos granulares, não apenas resposta final.

```rust
pub enum ReplEvent {
    SessionStarted { session_id: SessionId },
    VoiceListeningStarted,
    VoiceTranscriptPartial { text: String },
    VoiceTranscriptFinal { text: String },
    ScreenCaptured { source: CaptureSource, bytes: usize },
    OcrCompleted { chars: usize, language_hint: Option<String> },
    IntentDetected { intent: ReplIntent, confidence: f32 },
    PolicyEvaluated { policy: AssistancePolicy, allowed: bool },
    ModelSelected { model: String },
    TokenDelta { run_id: RunId, text: String },
    ToolStarted { name: String },
    ToolCompleted { name: String, status: ToolStatus },
    TtsQueued,
    TtsStarted,
    TtsCompleted,
    RunCompleted { run_id: RunId },
    Error { code: String, message: String },
}
```

## Intents do REPL

```rust
pub enum ReplIntent {
    AskTechnicalQuestion,
    ExplainScreen,
    ExplainCode,
    DebugCode,
    SolvePracticeQuestion,
    MultipleChoiceAssist,
    GenerateTestCases,
    ExplainTerminalError,
    SearchDocs,
    OpenApplication,
    OpenWebsite,
    ConfigureModel,
    ManageContext,
    AgenticCodeChange,
    Unknown,
}
```

## Comandos Estruturados

```rust
pub enum ReplCommand {
    Ask {
        text: String,
        context_policy: ContextPolicy,
    },
    CaptureAndExplain {
        mode: ScreenAssistMode,
        policy: AssessmentPolicy,
    },
    VoiceTurn {
        transcript_override: Option<String>,
    },
    OpenUi {
        mode: ReplMode,
    },
    SelectModel {
        model: ModelRef,
        role: ModelRole,
    },
    StopActiveRun,
    StopSpeaking,
}

pub enum ScreenAssistMode {
    ExplainVisibleScreen,
    ExplainCode,
    DebugError,
    MultipleChoice,
    SummarizeDocument,
}
```

`StopActiveRun` e `StopSpeaking` são requisitos de UX: o usuário precisa interromper geração ou áudio imediatamente quando o contexto mudar.

## IPC

### Curto prazo

Usar Unix socket já existente do daemon e adicionar requests:

```rust
pub enum VisionRequest {
    ReplCommand(ReplCommandJob),
    ReplSessionEvent(ReplSessionEventJob),
    ReplCapture(ReplCaptureJob),
    // existentes...
}
```

### Médio prazo

Criar camada Tauri:

```rust
#[tauri::command]
async fn repl_send_message(command: ReplCommand) -> Result<ReplRunHandle, ReplError>;

#[tauri::command]
async fn repl_capture_screen(mode: CaptureMode) -> Result<ScreenContext, ReplError>;

#[tauri::command]
async fn repl_set_policy(policy: AssistancePolicy) -> Result<(), ReplError>;
```

### Streaming

Para token streaming e logs:

- Tauri channels quando dentro do app.
- Unix socket framed events quando CLI/floating terminal externo.
- Frontend deve tratar eventos como append-only log e derivar estado por reducer.

## Assistência de Tela e Código

Pipeline:

1. Capturar screenshot.
2. Rodar OCR.
3. Detectar tipo de tela: IDE, terminal, navegador, assessment, múltipla escolha, documentação.
4. Extrair regiões: enunciado, opções, editor, saída, erro, botões.
5. Detectar linguagem de código.
6. Construir `ScreenUnderstandingContext`.
7. Aplicar política de integridade.
8. Escolher prompt e ação.
9. Responder em texto/voz.

```rust
pub struct ScreenUnderstandingContext {
    pub source_app: Option<String>,
    pub visible_text: String,
    pub detected_kind: ScreenKind,
    pub code_blocks: Vec<CodeBlock>,
    pub question: Option<QuestionBlock>,
    pub multiple_choice_options: Vec<ChoiceOption>,
    pub terminal_blocks: Vec<TerminalBlock>,
    pub confidence: f32,
}
```

## Política de Segurança

O REPL não deve dar ao frontend poder direto de executar shell. Toda ação passa pelo action registry do daemon.

Níveis:

- `Level0`: responder, explicar, resumir.
- `Level1`: abrir app/site/documentação.
- `Level2`: capturar tela, ler arquivos escolhidos.
- `Level3`: executar comandos allowlistados, alterar arquivos.
- `Level4`: bloqueado por padrão.

Para modo agentic, qualquer execução local deve exibir:

- comando proposto;
- razão;
- diretório;
- risco;
- timeout;
- saída esperada;
- botão de autorizar/cancelar.

## Baixa Latência

Metas iniciais:

- Abrir floating terminal: menos de 150 ms após atalho.
- Início de gravação: menos de 250 ms após UI visível.
- Transcrição curta local: menos de 1500 ms após fala.
- Primeira resposta textual: menos de 2500 ms para perguntas simples.
- Primeiro áudio: menos de 3500 ms quando TTS ligado.

Estratégias:

- Pré-carregar daemon e modelo selecionado.
- Manter Piper HTTP aquecido.
- Captura incremental de contexto.
- Streaming de tokens.
- Cache de OCR da última captura.
- Reducer leve no frontend.
- Evitar re-render de histórico inteiro.
- Serializar TTS, como já implementado no daemon.

## Dados Persistidos

Persistir:

- preferências de UI;
- modelo selecionado;
- histórico de sessões se usuário permitir;
- documentos explicitamente adicionados ao workspace;
- métricas anônimas locais de latência.

Não persistir por padrão:

- screenshots;
- áudio bruto;
- transcrições de assessment;
- tokens/API keys em texto plano;
- conteúdo de clipboard;
- respostas de provas/testes.

## Estrutura de Pastas Proposta

```text
apps/
  coddy/
    src-tauri/
    src/
      app/
      shared/
      features/
        terminal/
        desktop/
        voice/
        screen/
        model-manager/
        settings/
        assessment/
      styles/
      test/

crates/
  coddy-core/
    src/
      session.rs
      event.rs
      command.rs
      assessment.rs
      context.rs
      prompt.rs
```

## Decisões Arquiteturais

- **Coddy como interface, VisionClip como plataforma:** o usuário interage com `coddy`; captura, OCR, TTS e integrações continuam no daemon VisionClip.
- **Usar Tauri, não Electron:** mantém alinhamento com Rust, footprint menor e integração nativa.
- **xterm.js no modo terminal:** evita criar terminal do zero e oferece addons úteis.
- **Monaco apenas no advanced:** o modo simples deve ser leve; Monaco é poderoso, mas pesado para overlay rápido.
- **Rust mantém autoridade:** TypeScript renderiza e solicita; Rust valida, executa e persiste.
- **Policies antes de prompts:** o modelo não decide sozinho se pode responder diretamente.
