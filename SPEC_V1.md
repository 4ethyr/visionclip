# Especificação do MVP

## Objetivo

Serviço local para Linux que recebe uma captura de tela, executa uma ação de IA local e devolve resultado por clipboard, navegador ou áudio.

## Componentes

- `visionclip`: processo curto invocado por atalho ou comando de captura.
- `visionclip-daemon`: serviço residente, dono do clipboard e orquestrador principal.
- `visionclip-config`: utilitário de bootstrap e diagnóstico.
- `Ollama`: runtime do modelo local.
- `Piper HTTP`: sidecar local de TTS.

## Ações

- `CopyText`
- `ExtractCode`
- `TranslatePtBr`
- `Explain`
- `SearchWeb`

## Contratos principais

- IPC por socket Unix com `bincode`.
- Inferência via `POST /api/chat` do Ollama.
- TTS via Piper HTTP.
- Playback local via comando configurável.
- Captura via arquivo, comando externo ou backend nativo resolvido localmente.

## Decisões de arquitetura

- Clipboard mantido no daemon.
- Wayland deve ser `portal-first`, com fallback para backends nativos disponíveis no host.
- X11 pode usar `maim` ou `gnome-screenshot`, além de comando externo explícito.
- TTS desacoplado do core por HTTP.
