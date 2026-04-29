# VisionClip

VisionClip é um serviço local para Linux que transforma seus modelos locais em um assistente de AI, realiza pesquisas, traduz frases, abre programas e explica screenshots. O projeto combina captura de tela, inferência multimodal, integração com clipboard, pesquisa web, comandos de voz e resposta por áudio, com foco em privacidade, autonomia local e integração nativa com o desktop.

## O que o projeto entrega hoje

- `visionclip`: cliente curto para enviar uma imagem ao daemon por `--image`, `--capture-command` ou captura nativa automática.
- `visionclip-daemon`: serviço residente com socket Unix, integração com Ollama, clipboard e TTS.
- `visionclip-config`: utilitário de bootstrap, diagnóstico do host e listagem de modelos locais.
- Suporte a ações de `CopyText`, `ExtractCode`, `TranslatePtBr`, `Explain` e `SearchWeb`.
- Pipeline padrão com `gemma4:e2b` para OCR e raciocínio textual no mesmo stack local.
- `SearchWeb` agora gera a query, tenta enriquecer a resposta com scrape best-effort do Google e pode copiar um resumo inicial para o clipboard antes de abrir o navegador.
- Integração com Ollama via `/api/chat`, com retry automático quando o modelo não suporta `think`.
- Integração com Piper HTTP, com fallback de playback entre `paplay`, `pw-play` e `aplay`.
- Captura automática com resolução de backend via config: portal com `gdbus`, GNOME Shell Screenshot via D-Bus, `gnome-screenshot`, `grim` e `maim`.
- Configuração local em `~/.config/visionclip/config.toml`.

## Arquitetura resumida

1. `visionclip` recebe uma captura já existente, executa um comando externo ou resolve automaticamente um backend nativo de screenshot.
2. A imagem é enviada por socket Unix ao `visionclip-daemon`.
3. O daemon extrai texto com `infer.ocr_model` e envia esse texto para o modelo principal configurado no Ollama. No default atual, `gemma4:e2b` faz as duas etapas.
4. O resultado é pós-processado conforme a ação pedida.
5. A saída é enviada para clipboard, navegador ou TTS.

## Projeto Coddy

O **Coddy**, REPL visual/CLI com modo terminal flutuante, modo desktop app, voz, screen understanding e assistência segura para coding practice/assessments, foi separado para o repositório local `/home/aethyr/Documents/coddy`.

Enquanto a integração entre os projetos ainda existir, o daemon do VisionClip mantém uma camada local de compatibilidade de wire atrás da feature explícita `coddy-protocol`. O build padrão do VisionClip não depende do checkout Coddy. A documentação do Coddy foi movida para [../coddy/docs/repl/README.md](../coddy/docs/repl/README.md).

A fronteira atual entre os repositórios está registrada em [docs/repository-boundaries.md](docs/repository-boundaries.md).

## Status atual

Nesta etapa, o projeto passa a validar o fluxo principal com `gemma4:e2b` tanto para OCR quanto para `TranslatePtBr`, `Explain` e `SearchWeb`. O caminho multimodal puro continua suportado, mas o default foi mantido em `OCR -> Gemma -> Gemma` porque ele preservou melhor a qualidade do que a imagem direta e ficou mais rápido do que o stack com dois modelos. Em Wayland, o launcher tenta primeiro o portal de screenshot quando `prefer_portal = true`; se isso não estiver disponível, ele pode cair para outros backends compatíveis instalados no host.

## Requisitos do host

- Linux com sessão gráfica
- Rust toolchain
- Ollama instalado e ativo
- `gemma4:e2b`
- Piper HTTP para áudio real, se você quiser TTS fora dos mocks de teste
- Ferramentas nativas de desktop como `xdg-open`, `notify-send` e algum player de áudio suportado
- Para captura automática: `gdbus` com portal/serviço nativo do desktop, ou ferramentas como `gnome-screenshot`, `grim` ou `maim`
- Para observar a AI Overview renderizada no navegador em GNOME/Kali: GNOME Shell Screenshot via D-Bus, `gnome-screenshot`, `grim` ou `maim`

## Quick Start

```bash
cargo build --workspace

cp examples/config.toml ~/.config/visionclip/config.toml
visionclip-config init
visionclip-config doctor
visionclip-config models

# Em outro terminal
VISIONCLIP_CONFIG=/home/aethyr/Documents/visionclip/tools/visionclip-hybrid.toml visionclip-daemon

# Traduzir uma captura já salva
VISIONCLIP_CONFIG=/home/aethyr/Documents/visionclip/tools/visionclip-hybrid.toml visionclip --action translate_ptbr --image /caminho/captura.png --speak

# Captura nativa automática conforme o backend configurado
VISIONCLIP_CONFIG=/home/aethyr/Documents/visionclip/tools/visionclip-hybrid.toml visionclip --action explain

# Explicar uma captura gerada por um backend externo
VISIONCLIP_CONFIG=/home/aethyr/Documents/visionclip/tools/visionclip-hybrid.toml visionclip --action explain --capture-command 'maim -s -u'
```

## Scripts locais

Para subir uma stack local de teste com Piper HTTP, voz PT-BR, config persistida no repositório e daemon:

```bash
./scripts/start_local_stack.sh
```

Para testar `Explain`, `TranslatePtBr` e `SearchWeb` com TTS:

```bash
# Com captura automatica
./scripts/test_tts_flows.sh

# Com imagem fixa
./scripts/test_tts_flows.sh --image /caminho/captura.png
```

Para derrubar os processos iniciados pelo helper:

```bash
./scripts/stop_local_stack.sh
```

Defaults importantes desses helpers:

- usam `venv/bin/python` do proprio repositório para o Piper
- baixam `pt_BR-faber-medium` automaticamente se a voz ainda nao existir
- escrevem runtime, logs, pid files e config em `tools/runtime/local-stack/`
- usam `pw-play` como player padrao
- usam `gemma4:e2b` como modelo principal por padrao
- usam `gemma4:e2b` tambem como OCR por padrao
- configuram `capture_timeout_ms = 60000`

## Segredos locais

Arquivos `openrouter*` na raiz do repositório são tratados como locais e ignorados pelo Git. Antes de commitar, rode:

```bash
./scripts/guard_no_secrets.sh
```

Esse script falha se detectar padrões de chave OpenRouter em arquivos rastreados ou staged.

## Voz e agente local

O modo `--voice-agent` captura a fala, resolve uma intenção local simples e decide entre abrir aplicativo ou pesquisar na web. Ele é o caminho usado pelo atalho global instalado pelo script `scripts/install_gnome_voice_shortcut.sh`.

Para instalar o atalho global no GNOME:

```bash
cargo build --release --workspace --features gtk-overlay
install -Dm755 target/release/visionclip ~/.local/bin/visionclip
bash scripts/install_gnome_voice_shortcut.sh '<Shift>CapsLk'
```

O instalador configura `Super+F12` como atalho principal, `Super+Shift+F12` como fallback e `Super+Alt+V` como fallback alternativo para o mesmo wrapper. Ao acionar o atalho, o wrapper executa `visionclip --voice-agent --speak`, abre a overlay de escuta e grava o comando de voz.

O wrapper importa o ambiente gráfico do `systemd --user` antes de iniciar o binário, para que `DISPLAY`, `WAYLAND_DISPLAY`, `XDG_RUNTIME_DIR` e o barramento D-Bus estejam disponíveis quando o comando vier do GNOME. O instalador grava a tecla Super como `Mod4`, que é o nome de baixo nível usado pelo GTK/GNOME para esse modificador. Logs de acionamento ficam em `~/.local/state/visionclip/voice-shortcut.log`.

Para testar outro acelerador no GNOME, passe o binding desejado ao instalador. O alias `Shift+CapsLk` é normalizado para `<Shift>Caps_Lock`:

```bash
bash scripts/install_gnome_voice_shortcut.sh 'Shift+CapsLk'
```

Exemplos de teste sem microfone:

```bash
visionclip --voice-agent --voice-transcript 'Abra o terminal'
visionclip --voice-agent --voice-transcript 'Abra o VS Code'
visionclip --voice-agent --voice-transcript 'youtube'
visionclip --voice-agent --voice-transcript 'abra o site do LinkedIn'
visionclip --voice-agent --voice-transcript 'O que é JavaScript?'
```

Também é possível acionar a abertura segura de aplicativo diretamente:

```bash
visionclip --open-app terminal
visionclip --open-app vscode
```

O handler de abertura usa allowlists para casos conhecidos como terminal/navegador/configurações, resolução por arquivos `.desktop` com `gtk-launch`/`gio` e uma lista explícita de sites comuns que devem abrir no navegador padrão, como YouTube, Facebook e LinkedIn. O LLM não executa shell arbitrário.

## Diagnóstico e operação

Use `visionclip-config doctor` para verificar:

- caminho da configuração ativa
- socket do daemon
- sessão gráfica atual
- desktop atual
- backend e timeout de captura
- backends de screenshot expostos pelo `xdg-desktop-portal`
- disponibilidade do Ollama
- modelos locais expostos pelo runtime
- probe real de carregamento do modelo configurado
- reachability do Piper HTTP
- ferramentas nativas do host usadas pelo fluxo

Use `visionclip --doctor` para validar especificamente o fluxo operacional do cliente de voz:

- socket do daemon via healthcheck IPC
- overlay GTK no ambiente gráfico atual
- gravador nativo de microfone
- comando STT configurado
- player de TTS
- wrapper `~/.local/bin/visionclip-voice-search`
- bindings GNOME `Super+F12` e `Super+Shift+F12`

Use `visionclip-config models` para listar os modelos disponíveis no Ollama e ajustar `infer.model` com o nome exato do runtime. Nesta etapa, o default do projeto usa `model = "gemma4:e2b"`, `ocr_model = "gemma4:e2b"`, `thinking_default = ""` e `context_window_tokens = 8192`.

Quando nenhum `--image` ou `--capture-command` é informado, o launcher usa `capture.backend`. Em `auto`, o fluxo tenta portal com `gdbus` quando `prefer_portal = true` e, se necessário, cai para GNOME Shell Screenshot via D-Bus, `gnome-screenshot`, `grim` ou `maim`, conforme a sessão e os mecanismos disponíveis no host.

Em desktops Wayland via portal, a captura pode depender de uma confirmação explícita do usuário na janela do `xdg-desktop-portal`. Se esse diálogo não for concluído dentro do timeout configurado, o launcher retorna erro com o resumo dos backends de screenshot detectados para a sessão atual.

## TTS

Com Piper HTTP ativo, o daemon pode responder em áudio para `TranslatePtBr`, `Explain`, `SearchWeb`, `OpenApplication` e `OpenUrl` quando `--speak` estiver ligado.

Para `SearchWeb`, o daemon tenta falar o resumo enriquecido da busca quando esse material estiver disponivel; caso contrario, ele apenas confirma a abertura da pesquisa.

O tempo de síntese e reprodução do TTS é configurável em `[audio]`. O padrão atual permite respostas mais longas sem cortar a fala antes do final:

```toml
[audio]
request_timeout_ms = 60000
playback_timeout_ms = 120000
```

O daemon serializa a reprodução de áudio: se outra ação com `--speak` terminar enquanto uma fala ainda está tocando, a nova resposta aguarda a anterior terminar em vez de abrir outro player por cima.

## Busca enriquecida

O VisionClip tenta enriquecer `SearchWeb` com uma leitura inicial dos resultados do Google. Esse scrape e best-effort: quando houver bloco util equivalente a AI Overview/Visão geral criada por IA ou snippets organicos iniciais, o daemon monta contexto para clipboard e TTS.

Quando uma Visão geral criada por IA estiver disponível no HTML retornado, ela é tratada como contexto auxiliar gerado pelo Google/Gemini, não como verdade final. O daemon limpa ruído de interface, envia o texto extraído ao modelo local, gera uma resposta fundamentada somente nesse contexto, inclui o contexto capturado e lista fontes orgânicas iniciais para validação. Se a busca falhar, expirar, exigir CAPTCHA/autenticação ou o Google não devolver HTML útil, o fluxo cai de volta para o comportamento básico de abrir a consulta no navegador.

Se o Google renderizar a Visão geral criada por IA apenas dentro do navegador, o daemon inicia um listener curto após abrir a busca. Esse listener captura somente a tela visível da sessão do usuário com GNOME Shell Screenshot via D-Bus, `gnome-screenshot`, `grim` ou `maim`, aplica OCR local, extrai o bloco renderizado, pede ao modelo local uma resposta baseada nesse texto e fala essa resposta ao usuário. O arquivo temporário da captura fica em `XDG_RUNTIME_DIR/visionclip/rendered-search`, não em `/tmp`.

As opcoes desse fluxo ficam em `[search]` na configuracao:

```toml
[search]
enabled = true
base_url = "https://www.google.com/search"
fallback_enabled = true
fallback_base_url = "https://html.duckduckgo.com/html/"
request_timeout_ms = 10000
max_results = 3
open_browser = true
rendered_ai_overview_listener = true
rendered_ai_overview_wait_ms = 12000
rendered_ai_overview_poll_interval_ms = 3000
```

Exemplo de inicialização do Piper HTTP:

```bash
python3 -m piper.http_server -m <VOICE_NAME> --host 127.0.0.1 --port 5000
```

## systemd de usuário

O repositório inclui units em `deploy/systemd/` e um instalador auxiliar em `deploy/install-user.sh`.

```bash
./deploy/install-user.sh
systemctl --user daemon-reload
systemctl --user enable --now visionclip-daemon.service
```

## Limites atuais

- A captura via portal ainda precisa de validação manual ampla em diferentes desktops Wayland
- Em algumas sessões Wayland, o portal pode abrir ou aguardar confirmação do usuário antes de devolver a captura
- A overlay compacta já existe, mas ainda precisa de validação visual ampla em diferentes compositores e escalas de tela
- A qualidade do OCR ainda depende da captura e do modelo configurado; se a captura vier ruidosa, erros pequenos como `170 -> 17` ainda podem acontecer
- O fluxo de áudio real depende de um Piper HTTP ativo no host

## Licença

Este projeto é distribuído sob a GNU Affero General Public License v3.0. Consulte [LICENSE](LICENSE) para o texto completo.

Se você executar o VisionClip como serviço acessível por rede e modificar o código, a AGPLv3 exige que você disponibilize o código-fonte correspondente dessas modificações aos usuários desse serviço.

## Contribuindo

Contribuições da comunidade open source são bem-vindas. Issues, revisões técnicas, testes em diferentes ambientes Linux, melhorias de captura Wayland, novos fluxos de OCR/TTS e hardening operacional são especialmente úteis para o estágio atual do projeto.

Se você abrir um PR, priorize mudanças pequenas, testáveis e com contexto técnico claro. O objetivo é fazer do VisionClip uma base sólida para automação local de screenshots com IA no Linux.

Consulte também [CONTRIBUTING.md](CONTRIBUTING.md).

Em nome de R. Rodrigues.
