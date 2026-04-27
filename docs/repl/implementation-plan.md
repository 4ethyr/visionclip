# Plano de Implementação

## Fase 0: Fundamentos de Domínio

Objetivo: criar contratos antes de UI.

Entregáveis:

- `crates/coddy-core`
- tipos `ReplSession`, `ReplEvent`, `ReplCommand`
- tipos `AssessmentPolicy`, `ScreenKind`, `CodeAssistContext`
- testes unitários de serialização e policy gating

Critério de aceite:

- `cargo test -p coddy-core` passa.
- Nenhum tipo depende de Tauri ou React.
- `rg "aether_cli|AETHER_|Aether|SYSTEM_REPL"` não encontra uso público fora de notas de protótipo.

## Fase 1: Extensão do Daemon

Objetivo: daemon passa a entender comandos do Coddy.

Entregáveis:

- `VisionRequest::ReplCommand`
- eventos progressivos
- screen classification inicial
- endpoint de sessão
- integração com captura/OCR atual
- TTS serializado reaproveitado

Critério de aceite:

- CLI consegue enviar `coddy ask "explique a tela"` com transcript mockado.
- Testes cobrem `Practice`, `UnknownAssessment`, `RestrictedAssessment`.

## Fase 2: Frontend Base

Objetivo: app TypeScript com design tokens e shell inicial.

Entregáveis:

- `apps/coddy`
- Tauri + Vite + React + TypeScript + Tailwind
- tokens Coddy derivados dos protótipos legados Aether Terminal
- componentes base: Button, IconButton, GlassPanel, StatusBadge, PromptInput
- Vitest + Testing Library
- Playwright configurado

Critério de aceite:

- `pnpm typecheck`
- `pnpm test`
- `pnpm test:e2e`
- renderização visual do shell com snapshots básicos

## Fase 3: Floating Terminal

Objetivo: entregar o modo simples.

Entregáveis:

- janela flutuante Tauri
- xterm/transcript controlado
- input texto
- botão mic
- botão speak
- seletor de modelo
- opacity/blur controls
- streaming de resposta

Critério de aceite:

- abre por atalho;
- aceita texto;
- recebe resposta streaming;
- mic dispara fluxo de voz existente;
- speak respeita fila TTS;
- fundo transparente sem fundo preto.

## Fase 4: Screen Understanding para Código

Objetivo: entender prints de IDE/terminal/código.

Entregáveis:

- `ScreenKind` classifier
- extração de blocos de código
- linguagem por heurística + Tree-sitter
- detecção de erro/stack trace
- prompt `DebugCode`
- resposta em `Explain`, `Debug`, `Guide`

Critério de aceite:

- dataset local com prints de VS Code, terminal, navegador e assessment fake.
- precisão aceitável documentada.
- nenhum teste real de terceiros precisa ser usado.

## Fase 5: Assessment Assistant

Objetivo: múltipla escolha e coding practice com integridade.

Entregáveis:

- detecção de assessment signals
- policy selector
- multiple choice extractor
- prompt resolver permitido
- prompt guia restrito
- UI `PolicyBanner`
- logs de política por sessão

Critério de aceite:

- em `Practice`, responde alternativa e explicação.
- em `RestrictedAssessment`, não responde alternativa final.
- em `UnknownAssessment`, pede confirmação antes de resposta direta.

## Fase 6: Advanced Desktop App

Objetivo: app completo.

Entregáveis:

- `ReplMainView`
- `AgenticExecutionMode`
- `ContextWorkspace`
- `LocalModelManager`
- `ConfigurationSettingsModal`
- histórico de sessões
- model manager integrado com `ollama list`

Critério de aceite:

- navegação entre painéis.
- workspace aceita context files.
- model manager lista modelos locais.
- settings persistem em config.

## Fase 7: Agentic Code Workflows

Objetivo: permitir mudanças de código com aprovação.

Entregáveis:

- action registry UI
- risk badges
- command preview
- diff viewer Monaco
- autorização explícita
- execução allowlistada

Critério de aceite:

- nenhum comando shell arbitrário vindo do LLM executa.
- Level 2+ exige confirmação.
- logs auditáveis por run id.

## Backlog Técnico

- VAD real para reduzir latência de voz.
- Cache de OCR por screenshot hash.
- RAG local com Tantivy + embeddings.
- Importação de contexto de IDE aberta.
- Modo privado por sessão.
- Export de sessão em markdown.
- Perf overlay interno.
- Visual regression testing.

## Branches Sugeridas

- `feat/coddy-core-session-domain`
- `feat/coddy-floating-terminal`
- `feat/coddy-assessment-policy`
- `feat/coddy-screen-code-understanding`
- `feat/coddy-desktop-app-shell`
- `feat/coddy-model-manager`

## Ordem Recomendada

1. `coddy-core`
2. daemon events
3. frontend base
4. floating terminal
5. screen/code understanding
6. assessment policy
7. advanced app

Essa ordem reduz risco porque valida o domínio e a integração antes de investir pesado em UI avançada.
