# Pesquisa e Referências

Este documento consolida referências externas pesquisadas em 2026-04-27 para orientar a arquitetura do REPL do VisionClip.

## UI Desktop e IPC

### Tauri v2

Tauri v2 é adequado para o modo desktop app porque permite frontend em TypeScript com backend Rust, IPC por comandos e plugins nativos. A documentação de comandos mostra que funções Rust expostas com `#[tauri::command]` podem ser chamadas pelo frontend com `invoke`, retornando dados serializáveis. A documentação também recomenda canais para streaming de dados maiores ou progressivos.

Implicação para o VisionClip:

- Usar Tauri como shell desktop do REPL advanced.
- Manter o daemon Rust atual como serviço local e expor IPC via comandos Tauri ou Unix socket.
- Usar canais/eventos para streaming de tokens, logs, progresso de OCR, estado de voz e execução agentic.

Referência: https://v2.tauri.app/es/develop/calling-rust/

### Sidecars e processos

A documentação de sidecars do Tauri cobre empacotamento e execução de binários externos. Isso importa se o REPL precisar iniciar componentes como STT, daemon de modelos, helpers de OCR ou um bridge Node/TypeScript.

Implicação para o VisionClip:

- Preferir o daemon Rust existente como processo principal.
- Tratar STT/TTS/model managers como providers configuráveis.
- Se houver sidecar, declarar permissões explícitas e argumentos allowlistados.

Referência: https://tauri.app/develop/sidecar/

### Atalhos globais

O plugin de atalhos globais do Tauri registra atalhos e avisa que atalhos já usados por outros apps podem não disparar.

Implicação:

- O REPL pode usar Tauri global shortcut quando rodando como app.
- Para GNOME/Kali, manter fallback via `gsettings`/GNOME Media Keys, já usado no VisionClip.
- O doctor deve verificar tanto atalhos GNOME quanto atalhos Tauri quando o app desktop existir.

Referência: https://v2.tauri.app/reference/javascript/global-shortcut/

## Terminal, Código e REPL

### xterm.js

xterm.js tem API própria, addons oficiais e suporte a fit, search, web-links, serialize, Unicode e WebGL. A documentação mostra o padrão `Terminal.loadAddon`, que encaixa bem no modo terminal flutuante.

Implicação:

- Usar xterm.js no modo simples para renderização terminal-like.
- Adicionar `@xterm/addon-fit` para responsividade, `@xterm/addon-web-links` para links e `@xterm/addon-search` para histórico.
- Não usar shell real por padrão; renderizar uma experiência terminal-controlada pelo domínio do REPL.

Referências:

- https://xtermjs.org/docs
- https://xtermjs.org/docs/guides/using-addons/

### Monaco Editor

Monaco é o editor web que fornece modelos, editores e providers. A documentação destaca que modelos representam conteúdos de arquivos e providers adicionam recursos inteligentes.

Implicação:

- Usar Monaco no modo advanced para blocos de código e diffs editáveis.
- Modelar screenshots de IDE/assessment como `CodeContext`, não como arquivos reais por padrão.
- Futuramente, conectar providers LSP/RAG para hover, diagnostics e patches.

Referência: https://github.com/microsoft/monaco-editor

### Tree-sitter

Tree-sitter fornece parsing incremental, rápido e robusto mesmo com erro de sintaxe.

Implicação:

- Usar Tree-sitter no backend Rust para detectar linguagem e estrutura de código extraído por OCR.
- Reconciliar OCR + parser para recuperar blocos incompletos de IDE/assessment.
- Gerar contexto estruturado: imports, funções, classes, assinatura, testes visíveis, erros de compilação.

Referência: https://tree-sitter.github.io/tree-sitter/

## Voz e Áudio

### Web Speech API e SpeechSynthesis

A Web Speech API separa reconhecimento de fala (`SpeechRecognition`) e síntese (`SpeechSynthesis`). `SpeechSynthesis` expõe estado de fala, fila e cancelamento.

Implicação:

- No webview/Tauri, Web Speech pode ser fallback experimental.
- O caminho principal deve continuar local via `pw-record`/ASR configurado e Piper HTTP, porque já existe no daemon e oferece maior controle offline.
- O frontend deve refletir estados como `listening`, `transcribing`, `speaking`, `queued`, `cancelled`.

Referências:

- https://developer.mozilla.org/en-US/docs/Web/API/Web_Speech_API
- https://developer.mozilla.org/en-US/docs/Web/API/SpeechSynthesis

### getUserMedia

`getUserMedia()` solicita permissão para microfone e retorna um `MediaStream`, mas depende do contexto e permissões do ambiente web.

Implicação:

- No Tauri, avaliar captura nativa Rust/CPAL ou bridge com WebView.
- Para baixa latência em Linux, manter backend nativo por PipeWire (`pw-record`) e adicionar VAD no daemon.
- UI deve mostrar claramente quando microfone está ativo.

Referência: https://developer.mozilla.org/en-US/docs/Web/API/MediaDevices/getUserMedia

## Frontend, Animação e Testes

### Tailwind backdrop blur/opacidade

Tailwind tem utilitários oficiais para `backdrop-blur-*` e `backdrop-opacity-*`, úteis para a identidade glassmorphism do Coddy.

Implicação:

- Converter tokens de `repl_ui/aether_terminal/DESIGN.md` para `tailwind.config.ts`, renomeando a identidade final para Coddy.
- Usar CSS variables para opacidade, blur, borda e glow configuráveis por usuário.
- Separar tema de componentes.

Referências:

- https://tailwindcss.com/docs/backdrop-blur
- https://tailwindcss.com/docs/backdrop-filter-opacity

### React responsivo

React recomenda `startTransition`/`useTransition` para atualizações não bloqueantes e `useDeferredValue` para manter UI responsiva em buscas/renderizações pesadas.

Implicação:

- Usar transições para troca de painéis, histórico e filtros.
- Usar deferred value para busca em histórico/contexto.
- Evitar que streaming de tokens bloqueie input.

Referências:

- https://react.dev/reference/react/useTransition
- https://react.dev/reference/react/useDeferredValue
- https://react.dev/reference/react/useEffectEvent

### Playwright + TypeScript

Playwright suporta TypeScript e recomenda rodar `tsc --noEmit` junto aos testes, porque Playwright executa TS mas não substitui typecheck.

Implicação:

- TDD frontend com Vitest para domínio/hooks e Playwright para fluxos reais.
- Pipeline local: `pnpm typecheck`, `pnpm test`, `pnpm test:e2e`.

Referências:

- https://playwright.dev/docs/test-typescript
- https://playwright.dev/docs/test-components

## CLI Agents e Fluxos Agentic

### Codex CLI e Aider

Ferramentas como Codex CLI e Aider validam o padrão de agent CLI local, multimodal, com contexto de arquivos, modos de aprovação e integração git/testes.

Implicação:

- O REPL deve expor modos: `Ask`, `Guide`, `Code`, `Agentic`.
- Ações que alterem arquivos, executem comandos ou abram apps devem passar por action registry e confirmação.
- A UX deve mostrar plano, diffs, logs, autorização e resultado.

Referências:

- https://help.openai.com/en/articles/11096431-openai-codex-ci-getting-started
- https://aider.chat/docs/

## Assessments e Integridade

Plataformas de assessment possuem regras variáveis. CodeSignal documenta cenários onde buscas são restritas a sintaxe e afirma que, em algumas regras, IA não é permitida. HackerRank documenta malpractices como ajuda não autorizada, cópia de código e navegação indevida. Ao mesmo tempo, HackerRank e CodeSignal também possuem modos oficiais de AI-assisted assessments quando o recrutador habilita esse uso.

Implicação:

- O VisionClip precisa de um **Assessment Integrity Mode**.
- Em contexto de treino, estudo, open-book ou AI permitido, pode responder diretamente.
- Em assessment ativo sem permissão explícita, deve oferecer dicas conceituais, explicar enunciado, ajudar a entender erros e ensinar abordagem, mas não fornecer a resposta final ou código completo.
- A UI deve exibir estado de política: `Practice`, `Permitted AI`, `Restricted Assessment`, `Unknown`.

Referências:

- https://support.codesignal.com/hc/en-us/articles/360051960134-General-Coding-Assessment-GCA-Rules-and-Setup
- https://candidatesupport.hackerrank.com/articles/9579706989-malpractices-in-tests
- https://support.hackerrank.com/articles/1152916770-ai-assisted-tests
- https://codesignal.com/newsroom/press-releases/codesignal-launches-ai-assisted-coding-assessments-and-interviews-redefining-technical-hiring-in-the-ai-era/
