# Plano de Implementação

## MVP Vertical

O primeiro MVP deve provar o caminho mais crítico para a experiência do usuário antes de avançar para o modo desktop app completo:

```text
atalho global -> overlay listening -> transcript mock/real -> intent -> resposta textual -> TTS serializado -> logs/diagnóstico
```

Escopo do MVP:

- `coddy voice --overlay` instalado e acionável pelo desktop.
- Lock de concorrência para impedir duas sessões de voz simultâneas.
- Overlay transparente em estado `listening` antes de ASR/LLM.
- `coddy shortcuts test` para validar bind, overlay, lock, socket e ambiente gráfico.
- `coddy ask` com transcript textual para testar o router sem microfone.
- `SearchResultContext` mockado para validar síntese baseada em fatos.
- TTS reaproveitando a fila serializada existente.

Fora do MVP:

- modo advanced completo;
- edição agentic de arquivos;
- RAG persistente;
- Monaco/diff viewer;
- gerenciador visual de modelos.

Critério de saída do MVP:

- um usuário em GNOME/Kali pressiona o atalho, vê a overlay, fala uma pergunta curta, recebe texto e áudio sem concorrência;
- o mesmo fluxo passa com transcript mockado em teste automatizado;
- falha de atalho ou daemon gera diagnóstico acionável, não silêncio.

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

- `CoddyRequest::Command` no protocolo direto do REPL
- eventos progressivos
- `ShortcutTriggered`, `OverlayShown` e eventos de busca/contexto
- screen classification inicial
- endpoint de sessão
- integração com captura/OCR atual
- TTS serializado reaproveitado
- `CoddyShortcutBroker` com lock de sessão ativa
- `coddy doctor shortcuts`
- `coddy shortcuts install`
- `coddy shortcuts test`

Critério de aceite:

- CLI consegue enviar `coddy ask "explique a tela"` com transcript mockado.
- Testes cobrem `Practice`, `UnknownAssessment`, `RestrictedAssessment`.
- pressionar o atalho dispara overlay `listening` antes de ASR/LLM;
- segundo acionamento durante fala não cria duas vozes simultâneas;
- logs de atalho incluem binding, comando, run id e resultado.
- `coddy shortcuts test` valida overlay, lock, socket do daemon e ambiente gráfico sem chamar LLM.

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
- extração de `ScreenRegion` com bounding boxes, confiança e fonte
- linguagem por heurística + Tree-sitter
- detecção de erro/stack trace
- prompt `DebugCode`
- resposta em `Explain`, `Debug`, `Guide`

Critério de aceite:

- dataset local com prints de VS Code, terminal, navegador e assessment fake.
- precisão aceitável documentada.
- nenhum teste real de terceiros precisa ser usado.

## Fase 5: Web Search e AI Overview Visível

Objetivo: responder pesquisas usando contexto real extraído da busca e da página visível.

Entregáveis:

- `SearchResultContext`
- extrator de resultados orgânicos
- detector de região `AiOverview`
- `SearchExtractionPolicy`
- listener de renderização com timeout configurável
- fixtures de páginas/renderizações com e sem AI Overview
- prompt de síntese baseado em fontes
- fallback para busca tradicional quando AI Overview não existir

Critério de aceite:

- resposta menciona conteúdo extraído quando `ai_overview_text` existir;
- resposta não usa template se a extração falhar;
- fontes orgânicas são preservadas no contexto;
- não tenta contornar CAPTCHA, autenticação, paywall ou bloqueio técnico;
- TTS fala a síntese completa dentro do limite de contexto configurado.
- testes de fixture verificam que fatos específicos da AI Overview aparecem na resposta final.

## Fase 6: Assessment Assistant

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

## Fase 7: Advanced Desktop App

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

## Fase 8: Agentic Code Workflows

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
- `feat/coddy-shortcut-broker`
- `feat/coddy-voice-overlay-mvp`
- `feat/coddy-search-context`
- `feat/coddy-floating-terminal`
- `feat/coddy-assessment-policy`
- `feat/coddy-screen-code-understanding`
- `feat/coddy-desktop-app-shell`
- `feat/coddy-model-manager`

## Ordem Recomendada

1. `coddy-core`
2. broker de atalho + lock de concorrência
3. daemon events + `coddy shortcuts test`
4. overlay/listening MVP
5. `coddy ask` e transcript mockado
6. `SearchResultContext` + síntese mockada
7. frontend base
8. floating terminal
9. screen/code understanding
10. assessment policy
11. advanced app

Essa ordem reduz risco porque valida o domínio e a integração antes de investir pesado em UI avançada.
