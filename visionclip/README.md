# VisionClip

VisionClip é um serviço local para Linux que transforma screenshots em ações úteis com IA local. O projeto combina captura de tela, inferência multimodal, integração com clipboard, pesquisa web e resposta por áudio, com foco em privacidade, autonomia local e integração nativa com o desktop.

## O que o projeto entrega hoje

- `visionclip`: cliente curto para enviar uma imagem ao daemon por `--image` ou `--capture-command`.
- `visionclip-daemon`: serviço residente com socket Unix, integração com Ollama, clipboard e TTS.
- `visionclip-config`: utilitário de bootstrap, diagnóstico do host e listagem de modelos locais.
- Suporte a ações de `CopyText`, `ExtractCode`, `TranslatePtBr`, `Explain` e `SearchWeb`.
- Integração com Ollama via `/api/chat`, com retry automático quando o modelo não suporta `think`.
- Integração com Piper HTTP, com fallback de playback entre `paplay`, `pw-play` e `aplay`.
- Configuração local em `~/.config/visionclip/config.toml`.

## Arquitetura resumida

1. `visionclip` recebe uma captura já existente ou executa um comando externo de screenshot.
2. A imagem é enviada por socket Unix ao `visionclip-daemon`.
3. O daemon consulta o modelo local configurado no Ollama.
4. O resultado é pós-processado conforme a ação pedida.
5. A saída é enviada para clipboard, navegador ou TTS.

## Status atual

O projeto já valida o fluxo principal de inferência local com `gemma4:e2b`, incluindo diagnóstico do runtime, fallback de áudio e testes automatizados do workspace. O backend Wayland portal-first ainda não foi implementado; no momento, a captura é feita por arquivo ou por comando externo.

## Requisitos do host

- Linux com sessão gráfica
- Rust toolchain
- Ollama instalado e ativo
- Um modelo local compatível com visão, como `gemma4:e2b`
- Piper HTTP para áudio real, se você quiser TTS fora dos mocks de teste
- Ferramentas nativas de desktop como `xdg-open`, `notify-send` e algum player de áudio suportado

## Quick Start

```bash
cargo build --workspace

cp examples/config.toml ~/.config/visionclip/config.toml
visionclip-config init
visionclip-config doctor
visionclip-config models

# Em outro terminal
visionclip-daemon

# Traduzir uma captura já salva
visionclip --action translate_ptbr --image /caminho/captura.png --speak

# Explicar uma captura gerada por um backend externo
visionclip --action explain --capture-command 'maim -s -u'
```

## Diagnóstico e operação

Use `visionclip-config doctor` para verificar:

- caminho da configuração ativa
- socket do daemon
- sessão gráfica atual
- disponibilidade do Ollama
- modelos locais expostos pelo runtime
- probe real de carregamento do modelo configurado
- reachability do Piper HTTP
- ferramentas nativas do host usadas pelo fluxo

Use `visionclip-config models` para listar os modelos disponíveis no Ollama e ajustar `infer.model` com o nome exato do runtime. O default atual do projeto usa `gemma4:e2b` com `thinking_default = ""`.

## TTS

Com Piper HTTP ativo, o daemon pode responder em áudio para `TranslatePtBr`, `Explain` e `SearchWeb` quando `--speak` estiver ligado.

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

- Captura Wayland via portal ainda não implementada
- Overlay/UI compacta ainda não implementada
- OCR dedicado como fallback ainda não implementado
- O fluxo de áudio real depende de um Piper HTTP ativo no host

## Licença

Este projeto é distribuído sob a GNU Affero General Public License v3.0. Consulte [LICENSE](LICENSE) para o texto completo.

Se você executar o VisionClip como serviço acessível por rede e modificar o código, a AGPLv3 exige que você disponibilize o código-fonte correspondente dessas modificações aos usuários desse serviço.

## Contribuindo

Contribuições da comunidade open source são bem-vindas. Issues, revisões técnicas, testes em diferentes ambientes Linux, melhorias de captura Wayland, novos fluxos de OCR/TTS e hardening operacional são especialmente úteis para o estágio atual do projeto.

Se você abrir um PR, priorize mudanças pequenas, testáveis e com contexto técnico claro. O objetivo é fazer do VisionClip uma base sólida para automação local de screenshots com IA no Linux.

Em nome de R. Rodrigues.
