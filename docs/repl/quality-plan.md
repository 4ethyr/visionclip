# Plano de Qualidade, TDD e Observabilidade do Coddy

## Filosofia

O REPL mistura UI, voz, OCR, LLM, TTS e ações locais. Sem testes e métricas, bugs serão difíceis de reproduzir. A estratégia deve separar domínio puro, integração e E2E.

## Sequência TDD do MVP

Implementar o MVP em ciclos pequenos, sempre começando pelo teste:

1. `coddy-core`: teste de serialização para `ReplCommand::VoiceTurn`, `ShortcutDecision` e eventos de overlay.
2. Broker: teste de lock não bloqueante para `StartListening`, `IgnoredBusy` e `StopSpeakingAndListen`.
3. Daemon IPC: teste com socket fake recebendo `VoiceTurn` e emitindo `OverlayShown` antes de qualquer evento de ASR.
4. Overlay: teste de componente garantindo estado `listening` imediato e fundo transparente.
5. TTS: teste de fila garantindo que nova fala cancela/aguarda conforme `ShortcutConflictPolicy`.
6. Busca: teste com fixture `SearchResultContext` exigindo que a resposta contenha fatos do contexto e não texto genérico.
7. Doctor: teste de falha para bind ausente, binário ausente, socket indisponível e ambiente gráfico incompleto.

Nenhum ciclo do MVP deve depender de Google real, microfone real ou modelo LLM real. Esses recursos entram em testes manuais e integração controlada depois dos mocks.

## Pirâmide de Testes

### Rust Unit Tests

Cobrir:

- serialização de `ReplEvent`;
- reducer de sessão;
- policy gating;
- broker de atalho e concorrência de sessão ativa;
- decisão `ShortcutConflictPolicy`;
- classificação heurística de tela;
- classificação de `ScreenRegion`;
- extração de múltipla escolha;
- construção de `SearchResultContext`;
- detecção de `AiOverview` visível;
- síntese de busca ancorada em fatos das fixtures;
- normalização de OCR;
- validação de comandos;
- action registry.

Exemplo:

```rust
#[test]
fn restricted_assessment_blocks_final_answer() {
    let policy = AssessmentPolicy::RestrictedAssessment;
    let decision = evaluate_assistance(policy, RequestedHelp::SolveMultipleChoice);
    assert_eq!(decision.allowed, false);
    assert_eq!(decision.fallback, AssistanceFallback::ConceptualGuidance);
}
```

### TypeScript Unit Tests

Cobrir:

- reducers de UI;
- parsing de eventos streaming;
- componentes de status;
- seleção de modelo;
- policy banner;
- prompt input;
- mic state machine.

Exemplo:

```ts
it("keeps input enabled while token stream updates transcript", () => {
  const state = reducer(initialState, tokenDelta("hello"));
  expect(state.input.disabled).toBe(false);
});
```

### Component Tests

Com Testing Library:

- `FloatingTerminalWindow`
- `ModelSelector`
- `MicButton`
- `PolicyBanner`
- `AgentPlanPanel`
- `ContextWorkspace`

### E2E com Playwright

Fluxos:

- abre floating terminal;
- atalho global abre overlay antes de iniciar ASR;
- segundo atalho durante fala não cria execução concorrente;
- `coddy shortcuts test` falha com mensagem acionável quando bind, socket ou ambiente gráfico estiver inválido;
- digita pergunta;
- recebe streaming;
- ativa/desativa fala;
- grava voz mockada;
- pesquisa com AI Overview visível gera resposta baseada no contexto extraído;
- pesquisa sem AI Overview usa resultados orgânicos e informa fonte;
- pesquisa com CAPTCHA/bloqueio detectado não tenta contornar e retorna fallback seguro;
- policy unknown pede confirmação;
- restricted assessment não mostra resposta final;
- advanced mode navega entre painéis;
- model manager exibe modelos mockados.

## Fixtures

Criar fixtures locais:

```text
fixtures/repl/
  screenshots/
    multiple-choice-practice.png
    multiple-choice-restricted.png
    vscode-error-python.png
    terminal-rust-error.png
    browser-docs-js.png
    google-ai-overview-js.png
    google-no-ai-overview-nasa.png
  ocr/
    multiple-choice-practice.txt
    vscode-error-python.txt
    google-ai-overview-js.txt
  events/
    token-stream-basic.jsonl
    assessment-restricted.jsonl
    shortcut-listening.jsonl
    search-ai-overview.jsonl
```

Não usar screenshots reais de assessments privados em fixtures públicas.

## Mocks

### LLM Mock

Retorna respostas determinísticas por intent.

### OCR Mock

Retorna texto e bounding boxes simuladas.

### TTS Mock

Simula fila e garante que não há overlap.

### STT Mock

Entrada de áudio vira transcript predefinido.

### Daemon Mock

Servidor local que emite `ReplEvent` em JSONL ou canal Tauri fake.

### Search Extractor Mock

Retorna três cenários determinísticos: AI Overview visível, apenas resultados orgânicos e bloqueio/CAPTCHA. A asserção principal deve verificar que a resposta final contém fatos presentes na fixture e não contém frases genéricas quando o contexto existe.

## Testes Manuais Mínimos no Host GNOME/Kali

Executar antes de marcar o MVP como validado no host real:

```bash
coddy doctor shortcuts
coddy shortcuts test
coddy ask "o que é javascript?"
coddy voice --overlay --transcript "quem foi rousseau?"
coddy voice --overlay
```

Critérios:

- `coddy shortcuts test` abre overlay sem chamar LLM.
- `coddy voice --overlay --transcript` executa sem microfone real.
- o atalho configurado abre a overlay com fundo transparente.
- pressionar o atalho durante fala não sobrepõe áudio.
- logs mostram `run_id`, decisão de conflito e latências principais.

## Métricas

Métricas por run:

- `shortcut_to_window_ms`
- `window_to_listening_ms`
- `recording_ms`
- `transcription_ms`
- `capture_ms`
- `ocr_ms`
- `context_build_ms`
- `first_token_ms`
- `total_response_ms`
- `tts_first_audio_ms`
- `tts_total_ms`
- `policy_decision_ms`
- `screen_kind_confidence`

## Observabilidade

Adicionar request/run IDs em:

- UI events;
- daemon logs;
- LLM calls;
- OCR;
- TTS;
- action registry;
- policy decisions.

Formato recomendado:

```json
{
  "timestamp": "2026-04-27T10:00:00Z",
  "run_id": "uuid",
  "session_id": "uuid",
  "component": "repl.policy",
  "event": "policy_evaluated",
  "policy": "RestrictedAssessment",
  "allowed": false,
  "latency_ms": 3
}
```

## Testes de Performance

Benchmarks mínimos:

- reducer processa 1000 token events sem travar input;
- floating terminal abre em menos de 150 ms em caminho quente;
- caminho frio mostra overlay ou erro acionável em menos de 900 ms;
- renderização de 500 mensagens usa virtualização;
- TTS queue não sobrepõe duas falas;
- OCR cache evita reprocessamento de screenshot idêntico.

## Acessibilidade

Testar:

- navegação por teclado;
- foco visível;
- contraste;
- screen reader labels;
- reduced motion;
- estados de erro sem depender apenas de cor.

Ferramentas:

- Playwright accessibility snapshots.
- axe-core no frontend.

## Segurança

Testes obrigatórios:

- API key não aparece no DOM em texto claro.
- comandos shell arbitrários são recusados.
- URLs não-http são recusadas.
- extrator de busca não tenta contornar CAPTCHA, autenticação ou paywall.
- action Level 2+ exige confirmação.
- screenshots não são persistidos sem opt-in.
- policy `RestrictedAssessment` bloqueia resposta final.

## Definition of Done

Uma feature do REPL só está completa quando:

- tem testes unitários;
- tem pelo menos um teste de integração ou E2E se envolver UI/IPC;
- tem estados de loading/error/cancel;
- respeita policy gating;
- tem logs com run id;
- não bloqueia input durante streaming;
- foi testada com `cargo test`, `cargo clippy`, `pnpm typecheck`, `pnpm test` e E2E aplicável.
