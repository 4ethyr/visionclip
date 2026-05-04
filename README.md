# VisionClip

VisionClip é um assistente de AI local-first para Linux. Ele roda como daemon de usuário, conversa com um cliente CLI leve por Unix Socket e integra captura de tela, OCR, modelos locais via Ollama, busca web, comandos de voz, TTS local com Piper, abertura segura de aplicativos/URLs e runtime inicial de documentos.

O objetivo do projeto é evoluir para um assistente desktop multimodal no estilo Siri, mas com privacidade por padrão, execução local, validação de ferramentas, auditoria e integração real com ambientes Linux.

## Estado Atual

O que já funciona no repositório:

- `visionclip`: cliente CLI para captura, comandos por voz, abertura de apps/URLs, busca web e documentos.
- `visionclip-daemon`: daemon residente com IPC Unix Socket, integração com Ollama, Piper HTTP, clipboard, navegador e ações Linux.
- `visionclip-config`: bootstrap, diagnóstico do host, listagem/probe de modelos locais e verificação de Piper.
- Núcleo agentic inicial com `ToolRegistry`, `PermissionEngine`, `SessionManager`, `AuditLog` e validação de ações antes da execução.
- Provider local via Ollama atrás de `AiProvider`/`ProviderRouter`, com política `local_first` e dados sensíveis em `local_only`.
- Captura automática por `xdg-desktop-portal`, GNOME Shell D-Bus, `gnome-screenshot`, `grim` ou `maim`, conforme o ambiente.
- Ações de screenshot: copiar texto, extrair código, traduzir, explicar e pesquisar.
- Busca web com Google HTML best-effort, fallback DuckDuckGo HTML e resumo local quando há contexto útil.
- Voz: push-to-talk com STT configurável, detecção simples de idioma, resposta por TTS na língua detectada quando houver voz configurada.
- Abertura por voz de aplicativos, sites e documentos locais.
- Busca local de livros/documentos por título, inclusive comando em uma língua e título em outra, como `abra o livro Grey Hat Python`.
- Runtime de documentos local-first para TXT, Markdown e PDF textual: ingestão, perguntas, resumo, tradução e leitura incremental com cache de tradução/áudio.
- Persistência local em SQLite para documentos, chunks, sessões de leitura, progresso, traduções, embeddings opcionais, cache de áudio e eventos de auditoria.

## Segurança

O **Coddy**, REPL visual/CLI com modo terminal flutuante, modo desktop app, voz, screen understanding foi separado para o repositório coddy.

- O modelo local é o padrão; cloud providers ficam desligados.
- Dados sensíveis, OCR de tela, documentos e contexto local usam política `local_only`.
- Ferramentas são registradas e validadas por schema antes de executar.
- Ações de maior risco passam pelo `PermissionEngine`.
- O LLM não executa shell arbitrário.
- Abertura de documentos usa diretórios e extensões suportadas, e abre o caminho final via `xdg-open`/`gio`, sem shell gerado por modelo.
- API keys e tokens não devem ser gravados no repositório. O instalador lê `HF_TOKEN` do ambiente ou pede o token no terminal sem ecoar.

## Instalação Completa

O fluxo recomendado é o instalador completo:

```bash
git clone https://github.com/4ethyr/visionclip.git
cd visionclip
bash scripts/install_visionclip.sh
```

O script faz, em ordem:

- instala dependências do sistema quando reconhece `apt`, `dnf`, `pacman` ou `zypper`;
- instala Rust via `rustup` se `cargo` não existir;
- instala Ollama se estiver ausente e você autorizar;
- cria um venv Python local em `~/.local/share/visionclip/venv`;
- instala as dependências Python de `requirements.txt` no `venv` local do projeto;
- baixa vozes Piper em `~/.local/share/visionclip/piper-voices`;
- inicia ou conecta ao Ollama;
- baixa `gemma4:e2b` pelo Ollama para uso real do VisionClip;
- opcionalmente baixa/cacheia o modelo oficial `google/gemma-4-E2B-it` no Hugging Face usando seu token;
- gera `~/.config/visionclip/config.toml`, com backup se já existir;
- compila release com suporte GTK para fallback visual;
- instala binários em `~/.local/bin`;
- instala e sobe os serviços de usuário `piper-http.service` e `visionclip-daemon.service`;
- instala o atalho GNOME de voz quando `gsettings` está disponível;
- instala o indicador GNOME `visionclip-status@visionclip` para mostrar escuta/fala na barra superior;
- roda diagnósticos no final.

Durante a instalação, o `sudo` pode pedir sua senha para pacotes do sistema. O Hugging Face token pode ser informado por variável de ambiente ou digitado quando o script pedir:

```bash
HF_TOKEN=hf_xxx bash scripts/install_visionclip.sh
```

Para evitar o download/cache do Hugging Face e usar apenas Ollama:

```bash
bash scripts/install_visionclip.sh --skip-hf-download
```

Para modo não interativo:

```bash
HF_TOKEN=hf_xxx bash scripts/install_visionclip.sh --yes
```

Opções úteis:

```bash
bash scripts/install_visionclip.sh --help
bash scripts/install_visionclip.sh --model gemma4:e2b
bash scripts/install_visionclip.sh --skip-system-packages
bash scripts/install_visionclip.sh --skip-ollama-install
bash scripts/install_visionclip.sh --no-shortcut
bash scripts/install_visionclip.sh --no-start
```

## Verificação

Após instalar:

```bash
visionclip-config doctor
visionclip --doctor
systemctl --user status visionclip-daemon.service
systemctl --user status piper-http.service
```

Se o daemon estiver ativo, teste sem microfone:

```bash
visionclip --voice-agent --voice-transcript 'Abra o terminal' --speak
visionclip --voice-agent --voice-transcript 'Open the book Programming TypeScript' --speak
visionclip --voice-agent --voice-transcript 'abra o livro Grey Hat Python' --speak
visionclip --voice-agent --voice-transcript 'O que é JavaScript?' --speak
```

Teste captura/OCR:

```bash
visionclip --action explain --speak
visionclip --action translate_ptbr --speak
```

Em Wayland, a captura via portal pode abrir uma confirmação do sistema. Se ela expirar, o doctor mostra quais backends foram detectados.

## Uso Diário

Comandos comuns:

```bash
# Abrir apps e sites
visionclip --voice-agent --voice-transcript 'Abra o terminal' --speak
visionclip --voice-agent --voice-transcript 'Open YouTube' --speak
visionclip --open-app terminal

# Buscar na web
visionclip --voice-agent --voice-transcript 'Pesquise Rust async no Linux' --speak
visionclip --voice-agent --voice-transcript 'Who founded Apple?' --speak

# Abrir documentos por voz
visionclip --voice-agent --voice-transcript 'abra o livro Programming TypeScript' --speak
visionclip --voice-agent --voice-transcript 'Open my book Grey Hat Python' --speak

# Captura de tela
visionclip --action explain --speak
visionclip --action translate_ptbr --speak
visionclip --action extract_code
```

O atalho GNOME padrão instalado pelo script é `Super+F12`, com fallbacks `Super+Shift+F12` e `Super+Alt+V`. Ele chama `visionclip --voice-agent --speak`, não o modo de busca pura. Os logs do atalho ficam em:

```text
~/.local/state/visionclip/voice-shortcut.log
```

Se o STT retornar só ruído/filler, como `You` ou `you too`, o CLI bloqueia a busca para não abrir o navegador com uma query acidental. Ao iniciar uma nova gravação pelo atalho, playbacks TTS temporários do próprio VisionClip também são interrompidos para reduzir feedback do alto-falante no microfone.

## Documentos, RAG Local e Audiobook

Fluxo básico:

```bash
visionclip document ingest /caminho/livro.md
visionclip document ingest /caminho/livro.pdf

visionclip document ask <document_id> 'Qual é a ideia principal deste capítulo?'
visionclip document summarize <document_id>
visionclip document translate <document_id> --target-lang pt-BR
visionclip document read <document_id> --target-lang pt-BR

visionclip document pause <reading_session_id>
visionclip document resume <reading_session_id>
visionclip document stop <reading_session_id>
```

Idiomas alvo aceitos em leitura/tradução de documentos:

```text
pt-BR, en, es, zh, ru, ja, ko, hi
```

Notas:

- TXT e Markdown funcionam sem dependências extras.
- PDF textual usa `pdftotext`/`poppler-utils` ou `mutool`/`mupdf-tools`.
- PDF escaneado ainda depende de OCR de documento futuro.
- EPUB ainda pode ser aberto por voz como arquivo local, mas ingestão textual de EPUB ainda não está implementada.
- A leitura incremental usa backpressure, cache de tradução e cache de áudio quando habilitado.

## Modelos

O runtime padrão usa Ollama:

```toml
[infer]
backend = "ollama"
base_url = "http://127.0.0.1:11434"
model = "gemma4:e2b"
ocr_model = "gemma4:e2b"
embedding_model = ""
```

O instalador baixa `gemma4:e2b` via:

```bash
ollama pull gemma4:e2b
```

O download do Hugging Face é separado e serve como cache local dos pesos oficiais, não como backend direto do daemon nesta fase. Para isso, o script usa `google/gemma-4-E2B-it` e seu `HF_TOKEN`. Se o download falhar, aceite os termos do modelo na página do Hugging Face e rode o instalador novamente.

Para listar modelos locais:

```bash
visionclip-config models
```

## Voz e TTS

O instalador configura Piper HTTP local em:

```text
http://127.0.0.1:5000
```

Vozes baixadas por padrão:

```text
pt_BR-faber-medium
en_US-lessac-medium
es_ES-sharvard-medium
zh_CN-huayan-medium
ru_RU-ruslan-medium
hi_IN-pratham-medium
```

O daemon escolhe a voz pela língua detectada do comando em `OpenApplication`, `OpenUrl`, `OpenDocument` e `SearchWeb`; para documentos, usa o idioma alvo da leitura/tradução.

Japonês e coreano são aceitos como idiomas de comando/documento, mas você precisa instalar uma voz Piper compatível ou plugar outro provider TTS local para pronúncia natural nesses idiomas.

No GNOME, o fluxo principal de feedback visual é o indicador de barra `visionclip-status@visionclip`. Ele lê `~/.local/state/visionclip/status.json`, mostra a animação compacta enquanto o microfone grava e troca para um ícone de stop durante a fala por TTS. Clicar no stop executa:

```bash
visionclip --stop-speaking
```

O overlay central antigo fica como fallback legado; novas instalações usam `ui.overlay = "panel"` e `voice.overlay_enabled = false`.

Em sessões GNOME Wayland, extensões copiadas durante a sessão podem não aparecer imediatamente em `gnome-extensions list`. Se `gnome-extensions enable visionclip-status@visionclip` responder `Extension ... does not exist`, encerre a sessão e entre novamente; o instalador já deixa o UUID marcado em `org.gnome.shell enabled-extensions`.

## Configuração

Arquivo principal:

```text
~/.config/visionclip/config.toml
```

Diretórios locais usados pelo instalador:

```text
~/.local/bin/visionclip
~/.local/bin/visionclip-daemon
~/.local/bin/visionclip-config
~/.local/share/visionclip/
~/.local/state/visionclip/
```

Serviços:

```bash
systemctl --user restart visionclip-daemon.service
systemctl --user restart piper-http.service
systemctl --user journalctl -u visionclip-daemon.service -f
systemctl --user journalctl -u piper-http.service -f
```

## Desenvolvimento

Build e validação:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features
cargo test --workspace
scripts/guard_no_secrets.sh
```

Build release manual:

```bash
cargo build --release --workspace --features gtk-overlay
install -Dm755 target/release/visionclip ~/.local/bin/visionclip
install -Dm755 target/release/visionclip-daemon ~/.local/bin/visionclip-daemon
install -Dm755 target/release/visionclip-config ~/.local/bin/visionclip-config
```

Stack local de desenvolvimento, sem instalar systemd:

```bash
scripts/start_local_stack.sh
scripts/test_tts_flows.sh
scripts/stop_local_stack.sh
```

## Limites Atuais

- A UI desktop completa em Electron/React ainda não está implementada.
- Wake word ainda não é prioridade; o fluxo atual é push-to-talk/atalho.
- STT usa faster-whisper configurável, com filtros contra transcrições curtas acidentais, mas ainda não há runtime de voz streaming completo.
- TTS usa Piper HTTP e player externo; o atalho interrompe playbacks temporários antes de gravar, mas o `AudioRuntime` controlável ainda está em evolução.
- Cloud providers estão modelados na configuração, mas não executam chamadas nesta fase.
- Busca vetorial com `sqlite-vec` ainda não está conectada; embeddings locais são opcionais.
- OCR de PDF escaneado e ingestão EPUB ainda são próximos passos.
- Comandos sensíveis como VPN/rede/e-mail continuam exigindo confirmação/política antes de execução completa.

## Referências Externas

- Gemma 4, Google DeepMind: https://deepmind.google/models/gemma/gemma-4/
- Gemma 4 E2B no Hugging Face: https://huggingface.co/google/gemma-4-E2B-it
- Gemma 4 no Ollama: https://ollama.com/library/gemma4
- Piper TTS: https://github.com/rhasspy/piper
- Piper voices: https://huggingface.co/rhasspy/piper-voices

## Licença

Este projeto é distribuído sob a GNU Affero General Public License v3.0. Consulte [LICENSE](LICENSE) para o texto completo.

Se você executar o VisionClip como serviço acessível por rede e modificar o código, a AGPLv3 exige que você disponibilize o código-fonte correspondente dessas modificações aos usuários desse serviço.

## Contribuindo

Contribuições são bem-vindas. Priorize mudanças pequenas, testáveis e com contexto técnico claro.

Consulte também [CONTRIBUTING.md](CONTRIBUTING.md).

Em nome de R. Rodrigues.
