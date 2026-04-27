# Plano de Qualidade, TDD e Observabilidade do Coddy

## Filosofia

O REPL mistura UI, voz, OCR, LLM, TTS e ações locais. Sem testes e métricas, bugs serão difíceis de reproduzir. A estratégia deve separar domínio puro, integração e E2E.

## Pirâmide de Testes

### Rust Unit Tests

Cobrir:

- serialização de `ReplEvent`;
- reducer de sessão;
- policy gating;
- classificação heurística de tela;
- extração de múltipla escolha;
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
- digita pergunta;
- recebe streaming;
- ativa/desativa fala;
- grava voz mockada;
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
  ocr/
    multiple-choice-practice.txt
    vscode-error-python.txt
  events/
    token-stream-basic.jsonl
    assessment-restricted.jsonl
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
- floating terminal abre em menos de 150 ms em build release;
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
