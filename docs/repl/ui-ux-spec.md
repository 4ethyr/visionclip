# Especificação UX/UI do Coddy

## Leitura dos Protótipos `repl_ui`

Os arquivos em `repl_ui` definem uma linguagem visual originalmente chamada **Aether Terminal**. Essa nomenclatura deve ser tratada como referência de protótipo. A marca final da experiência REPL/CLI será **Coddy**.

Artefatos analisados:

- `aether_terminal/DESIGN.md` como referência visual legada
- `floating_terminal_coding_interaction/code.html`
- `floating_terminal_model_selection/code.html`
- `repl_main_view/code.html`
- `agentic_execution_mode/code.html`
- `context_workspace/code.html`
- `local_model_manager/code.html`
- `configuration_settings_modal/code.html`

## Identidade Visual

### Direção

A UI do Coddy deve parecer um cockpit técnico, com glassmorphism, linhas finas, baixa opacidade e foco em leitura. O visual não deve ser genérico nem parecer dashboard SaaS comum.

Palavras-chave:

- translúcido;
- técnico;
- cinematográfico;
- calmo;
- responsivo;
- agentic;
- local-first.

### Naming na Interface

Usar:

- `Coddy`
- `Coddy Terminal`
- `Coddy Core`
- `Coddy Workspace`
- `Coddy Model Manager`
- `Coddy Settings`

Evitar:

- `Aether`
- `AETHER_CORE`
- `AETHER_TERMINAL`
- `SYSTEM_REPL` como marca visível
- `aether_cli`

`SYSTEM_REPL` pode sobreviver apenas como label técnico interno em logs ou mocks, se necessário, mas não deve ser usado em telas finais.

### Tokens

Tokens principais vindos de `DESIGN.md`:

- `surface`: `#131313`
- `surface-container`: `#201f1f`
- `surface-container-high`: `#2a2a2a`
- `on-surface`: `#e5e2e1`
- `on-surface-variant`: `#b9cacb`
- `primary-container`: `#00f0ff`
- `primary-fixed-dim`: `#00dbe9`
- `secondary-container`: `#b600f8`
- `secondary`: `#ebb2ff`
- `outline`: `#849495`
- `outline-variant`: `#3b494b`

Tipografia:

- Headings: `Space Grotesk`
- Body: `Manrope`
- Labels: `Inter`
- Terminal/código: `JetBrains Mono`

## Tailwind

Criar `tailwind.config.ts` com tokens reais, evitando duplicação inline dos protótipos.

Exemplo:

```ts
export default {
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        surface: "#131313",
        "surface-container": "#201f1f",
        "primary-container": "#00f0ff",
        "secondary-container": "#b600f8",
      },
      fontFamily: {
        display: ["Space Grotesk", "sans-serif"],
        body: ["Manrope", "sans-serif"],
        label: ["Inter", "sans-serif"],
        mono: ["JetBrains Mono", "monospace"],
      },
    },
  },
};
```

## Modo Simples: Floating Terminal

### Estrutura

Componentes:

- `FloatingTerminalWindow`
- `FloatingTerminalHeader`
- `ModelSelector`
- `TerminalTranscript`
- `TerminalPrompt`
- `MicButton`
- `SpeakToggle`
- `OpacityControl`
- `BlurControl`

### Comportamento

O terminal flutuante deve:

- abrir com atalho;
- exibir estado `listening` imediatamente após o atalho, antes de qualquer resposta do modelo;
- ficar centralizado ou próximo ao cursor, configurável;
- ser always-on-top quando permitido;
- ter fundo transparente real;
- permitir opacidade entre `0.35` e `0.95`;
- permitir blur entre `0px` e `40px`;
- aceitar texto e voz;
- exibir streaming de resposta;
- indicar estado do agente;
- permitir cancelar fala/transcrição/execução.

### Estados Visuais

- `idle`: cursor piscando, input ativo.
- `listening`: mic com halo pulsante e waveform minimalista.
- `transcribing`: linha de status com spinner técnico.
- `thinking`: skeleton ou "thinking trace" curto.
- `streaming`: tokens aparecem incrementalmente.
- `speaking`: ícone de alto-falante com pulso.
- `busy`: sessão ativa detectada; mostrar ação tomada, como "fala interrompida" ou "atalho ignorado".
- `blocked`: badge de política com explicação curta.
- `error`: vermelho desaturado, sem alarmismo visual.

### Regras de Animação

- Entrada da janela: scale `0.96 -> 1`, opacity `0 -> 1`, blur `8px -> 0`.
- Streaming: cursor terminal e fade-in por linha.
- Listening: halo cyan radial, 2 s ease-in-out infinite.
- Thinking: rotação lenta em ícone `data_usage`, no máximo 3 s por ciclo.
- Preferir transform/opacity; evitar layout thrashing.
- Respeitar `prefers-reduced-motion`.

## Modo Advanced: Desktop App

### Estrutura

Componentes:

- `ReplShell`
- `SideNav`
- `TopBar`
- `SessionTimeline`
- `AgentPlanPanel`
- `TerminalExecutionPanel`
- `WorkspaceContextPanel`
- `ModelManager`
- `SettingsModal`
- `HistoryPanel`
- `PolicyBanner`

### Layout

Desktop:

- Sidebar: `240px`.
- Topbar: `48px`.
- Canvas: grid flexível.
- Painéis principais: conversa + terminal/contexto.
- Safe area: `24px`.

Tablet:

- Sidebar colapsável.
- Painéis empilháveis.
- Bottom prompt fixo.

Mobile:

- Modo advanced deve degradar para single-pane.
- Floating terminal deve ser o modo preferencial.

## Componentes por Protótipo

### Floating Terminal: Coding Interaction

Conservar:

- top bar compacta;
- cards de usuário/IA;
- bloco de código com copy;
- prompt arredondado;
- aurora gradient discreta.

Melhorar:

- adicionar mic e speak toggle;
- trocar HTML estático por xterm/transcript controlado;
- remover background ilustrativo quando em overlay real;
- adicionar controle de opacity/blur.

### Floating Terminal: Model Selection

Conservar:

- dropdown com modelos cloud/local;
- status dot por provider;
- ícone de cloud/memory/api.

Melhorar:

- exibir latência média;
- indicar modelo atual de OCR, LLM e STT;
- bloquear modelos indisponíveis com tooltip acionável;
- integrar `visionclip-config models`.

### REPL Main View

Conservar:

- sidebar Coddy Core;
- timeline de sessão;
- cards agentic;
- input flutuante com mic.

Melhorar:

- separar mensagens, runs e tool events;
- adicionar `PolicyBanner`;
- permitir alternância `Ask / Guide / Code / Agentic`;
- adicionar estado de TTS enfileirado/ativo.

### Agentic Execution Mode

Conservar:

- `Plan of Attack`;
- painel terminal;
- botão `Authorize Execution`.

Melhorar:

- exibir risk level;
- mostrar comando allowlistado;
- exigir confirmação para Level 2+;
- permitir copiar plano sem executar;
- registrar audit log local.

### Context Workspace

Conservar:

- dropzone;
- pills de arquivos/contextos;
- ambiente selecionado.

Melhorar:

- suportar screenshots recentes;
- suportar snippets de código detectados;
- mostrar token budget;
- permitir privacidade por item;
- expirar contexto sensível automaticamente.

### Local Model Manager

Conservar:

- vitals CPU/VRAM/RAM;
- lista de daemons;
- console de logs.

Melhorar:

- integrar Ollama list/pull;
- mostrar modelo carregado, contexto, quantização e tokens/s;
- botão de warmup;
- health checks;
- logs filtráveis por request id.

### Configuration Modal

Conservar:

- seções `Neural Links`, `Local Execution`, `Parameters`, `Environment`.

Melhorar:

- nunca renderizar API keys reais em value;
- usar secret storage;
- validar paths;
- separar config global e config por perfil.

## Acessibilidade

Requisitos:

- Contraste mínimo adequado para texto essencial.
- Navegação por teclado em todos os controles.
- Focus ring visível em cyan.
- Labels reais para ícones.
- `prefers-reduced-motion`.
- Leitores de tela para status: `listening`, `thinking`, `speaking`, `blocked`.
- Não depender apenas de cor para status.

## Diagnóstico de Atalhos

`coddy doctor shortcuts` deve ter uma saída legível na UI e na CLI:

- bind instalado;
- comando resolvido;
- binário executável;
- socket do daemon acessível;
- ambiente gráfico detectado;
- último acionamento com timestamp;
- correção sugerida quando algo falhar.

Na UI, falhas de atalho devem aparecer como cards acionáveis, não apenas logs.

## Conteúdo e Tom

O agent CLI deve responder de forma curta quando a ação for simples:

```text
Abrindo o terminal.
```

Para dúvidas técnicas:

```text
O erro ocorre porque a promise não está sendo aguardada. O fluxo correto é...
```

Para assessment restrito:

```text
Posso ajudar com a abordagem e os conceitos, mas não posso indicar a resposta final se esta for uma avaliação ativa sem permissão de IA.
```

## Critérios de Aceite UI

- Floating terminal abre em menos de 150 ms após atalho em host com daemon ativo.
- Atalho global funciona com app fechado via broker residente ou fallback GNOME Media Keys.
- Se o atalho falhar, `coddy doctor shortcuts` mostra causa e correção sugerida.
- Fundo transparente funciona em GNOME/Kali no modo overlay/app quando suportado.
- Opacity e blur são configuráveis.
- Input não perde foco durante streaming.
- Microfone mostra estado em tempo real.
- TTS nunca sobrepõe falas.
- Modo advanced renderiza em 1366x768 sem overflow crítico.
- Todos os principais fluxos têm teste de componente e E2E.
