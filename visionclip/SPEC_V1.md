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

## Decisões de arquitetura

- Clipboard mantido no daemon.
- Wayland deve ser `portal-first` em evolução futura.
- X11 pode usar captura por comando externo no MVP.
- TTS desacoplado do core por HTTP.
