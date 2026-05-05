# VisionClip

VisionClip é um assistente de AI local-first para Linux. Ele roda como daemon de usuário, conversa com um cliente CLI leve por Unix Socket e integra captura de tela, OCR, modelos locais via Ollama, busca web, comandos de voz, TTS local com Piper, abertura segura de aplicativos/URLs e runtime inicial de documentos.

O objetivo do projeto é evoluir para um assistente desktop multimodal no estilo Siri, mas com privacidade por padrão, execução local, validação de ferramentas, auditoria e integração real com ambientes Linux.

### Após a instalação, você conseguirá utilizar comandos:

Alt+Space para abrir o buscador
Windows+Space / Super+Space para abrir comando de voz
Alt+1 Para captura de tela com explicação
Alt+2 Para captura de tela e tradução de texto, e assim por diante.

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

Por padrão, o listener visual de Google AI Overview renderizada fica desligado (`search.rendered_ai_overview_listener = false`). Esse listener fazia capturas repetidas da tela após buscas abertas no navegador e podia causar flashes/quadrados brancos durante a fala do TTS em alguns ambientes GNOME/Wayland.

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
bash scripts/install_visionclip.sh --enable-wake-listener
```

## Verificação

Após instalar:

```bash
visionclip-config doctor
visionclip --doctor
systemctl --user status visionclip-daemon.service
systemctl --user status piper-http.service
systemctl --user status visionclip-wake-listener.service
```

O `visionclip-config doctor` limita o probe direto do modelo Ollama a 20 segundos. Se ele reportar `Configured model probe: failed (timed out...)`, o serviço ainda pode estar saudável, mas o modelo local demorou demais para responder ao diagnóstico curto.

Se o daemon estiver ativo, teste sem microfone:

```bash
visionclip --voice-agent-dry-run --voice-transcript 'Por favor, abra o terminal.'
visionclip --voice-agent-dry-run --voice-transcript 'Key, quem foi Steve Jobs?'
visionclip --voice-agent-dry-run --voice-transcript 'Open the book, Programming TypeScript.'
visionclip --voice-agent-dry-run --voice-transcript 'Pesquise, por Rust async no Linux'
visionclip --voice-agent --voice-transcript 'Key, Abra o terminal' --speak
visionclip --voice-agent --voice-transcript 'Key, Open the book Programming TypeScript' --speak
visionclip --voice-agent --voice-transcript 'Key, abra o livro Grey Hat Python' --speak
visionclip --voice-agent --voice-transcript 'Key, O que é JavaScript?' --speak
```

Teste captura/OCR:

```bash
visionclip --action explain --speak
visionclip --action translate_ptbr --speak
```

Em Wayland, a captura via portal pode abrir uma confirmação do sistema. Se ela expirar, o doctor mostra quais backends foram detectados.

Para evitar artefatos visuais, o VisionClip nunca inicia capturas renderizadas de busca enquanto uma resposta por voz está tocando, mesmo se uma configuração antiga ainda tiver `search.rendered_ai_overview_listener = true`.

## Uso Diário

Comandos comuns:

```bash
# Abrir apps e sites
visionclip --voice-agent-dry-run --voice-transcript 'Abra o terminal'
visionclip --voice-agent --voice-transcript 'Key, Abra o terminal' --speak
visionclip --voice-agent --voice-transcript 'Open YouTube' --speak
visionclip --open-app terminal

# Buscar na web
visionclip --voice-agent-dry-run --voice-transcript 'Pesquise, por Rust async no Linux'
visionclip --voice-agent --voice-transcript 'Pesquise Rust async no Linux' --speak
visionclip --voice-agent --voice-transcript 'Key, who founded Apple?' --speak

# Abrir documentos por voz
visionclip --voice-agent-dry-run --voice-transcript 'Open the book, Programming TypeScript.'
visionclip --voice-agent --voice-transcript 'abra o livro Programming TypeScript' --speak
visionclip --voice-agent --voice-transcript 'Key, Open my book Grey Hat Python' --speak

# Busca local indexada
visionclip locate docker-compose.yml
visionclip search 'architecture notes kind:document'
visionclip grep 'auth middleware' ./src
visionclip index status
visionclip index audit

# Captura de tela
visionclip --action explain --speak
visionclip --action translate_ptbr --speak
visionclip --action extract_code
```

A busca local roda dentro do `visionclip-daemon`, usa SQLite como catalogo inicial e aplica denylist forte para secrets. A UI Spotlight GTK é acionada por `Alt+Space` quando o atalho GNOME é instalado, e também pode ser aberta com `visionclip --search-overlay` em binarios compilados com `--features gtk-overlay`; veja [docs/search-subsystem.md](docs/search-subsystem.md).

A overlay mantém uma única instância por sessão gráfica: se você acionar o atalho enquanto ela já está aberta, a janela existente é reapresentada. Clique fora da área da busca, `Alt+Tab` para outra janela ou `Esc` fecham a overlay. Resultados longos são renderizados dentro de uma lista rolável com clipping no painel Liquid Crystal, evitando que documentos ou apps vazem para fora do layout arredondado.

Por padrão, a search usa o preset `liquid_crystal`, inspirado nos controles do Aether CSS: superfície translúcida escura, brilho espectral, bordas luminosas e barra de processamento cyan. Os presets configuráveis em `[ui.search_overlay]` são:

```text
liquid_crystal, liquid_glass, liquid_glass_advanced, aurora_gel, crystal_mist,
fluid_amber, frost_lens, ice_ripple, mercury_drop, molten_glass,
nebula_prism, ocean_wave, plasma_flow, prisma_flow, silk_veil, glass, glassmorphism,
frosted, bright_overlay, dark_overlay, dark_glass, high_contrast, vibrant,
desaturated, monochrome, vintage, inverted, color_shifted, animated_glass,
accessible_glass, neumorphism, neumorphic_pressed, neumorphic_concave,
neumorphic_colored, neumorphic_accessible
```

Exemplo de ajuste visual:

```toml
[ui.search_overlay]
glass_style = "liquid_crystal"
panel_opacity = 0.04
corner_radius_px = 28
border_opacity = 0.30
shadow_intensity = 0.28
highlight_intensity = 0.42
refraction_strength = 0.86
chromatic_aberration = 0.28
liquid_noise = 0.52
```

O agente também aceita o prefixo falado `Key` antes do comando, com som de `K` em inglês: `Key, quem foi Steve Jobs?`, `Key, abra o terminal` ou `Key, abra o livro Black Hat Python`.

Wake word contínua é opcional por privacidade. Para acionar o agente apenas falando `Key`, instale ou reinstale com:

```bash
bash scripts/install_visionclip.sh --enable-wake-listener
```

Isso habilita `visionclip-wake-listener.service`, um listener local que grava janelas curtas de áudio, roda STT local e só executa o agente quando o transcript começa com `Key`/`K`/variações comuns de ASR. Ao detectar `Key`, o indicador de barra passa para `listening`; se você disser apenas `Key`, ele abre uma segunda janela de escuta para o comando seguinte.

Por padrão, o listener ignora ativações enquanto existir playback ativo no PipeWire/PulseAudio, reduzindo disparos vindos de YouTube, música ou vídeos. Esse bloqueio usa `pactl list sink-inputs`, é habilitado por `wake_block_during_playback = true` e pode ser desativado somente se você aceitar esse risco:

```toml
[voice]
wake_block_during_playback = false
```

Também existe um gate local de locutor para reduzir ativações por áudio externo sem desligar a proteção de playback. Primeiro grave um perfil de voz local:

```bash
visionclip voice enroll --samples 3 --label main
visionclip voice status
```

Depois habilite a verificação no `~/.config/visionclip/config.toml`:

```toml
[voice]
speaker_verification_enabled = true
speaker_verification_threshold = 0.72
speaker_verification_min_samples = 3
```

Com um perfil válido, o wake listener pode continuar ouvindo durante YouTube/música, mas só aceita o comando `Key` quando a amostra capturada se parece com o perfil cadastrado. O perfil fica em `~/.local/share/visionclip/voice-profile.json`, contém apenas vetores acústicos derivados e pode ser removido com `visionclip voice clear`. Isso é um filtro de conveniência local, não autenticação biométrica forte.

Os atalhos GNOME padrão instalados pelo script são:

```text
Super+Space -> agente de voz
Alt+1       -> captura + explain
Alt+2       -> captura + translate_ptbr
Alt+3       -> pesquisa por voz
Alt+4       -> modo de voz para leitura de livro
Alt+5       -> modo de voz para leitura/tradução de livro
Alt+Space   -> busca local / Search Overlay
```

O GNOME custom-keybindings não suporta chording real `Super+Space+1`; por isso o instalador usa `Alt+1..5` para os modos derivados. Os logs dos atalhos ficam em:

```text
~/.local/state/visionclip/voice-shortcut.log
```

Se o STT retornar só ruído/filler, como `uh` ou `thank you`, o CLI bloqueia a busca para não abrir o navegador com uma query acidental. Variações comuns de ASR para YouTube, como `you too`, `you to` e `you two`, são tratadas como aliases de YouTube. Ao iniciar uma nova gravação pelo atalho, playbacks TTS temporários do próprio VisionClip também são interrompidos para reduzir feedback do alto-falante no microfone.

Use `--voice-agent-dry-run` para depurar comandos de voz sem executar ações. O comando imprime a intenção resolvida, idioma detectado e slots extraídos, por exemplo `intent=open_document`, `language=en` e `query=Programming TypeScript`.

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
dii_pt-BR
en_US-lessac-medium
es_ES-sharvard-medium
zh_CN-huayan-medium
ru_RU-ruslan-medium
hi_IN-pratham-medium
```

`dii_pt-BR` é a voz feminina pt-BR customizada usada como padrão. O daemon escolhe a voz pela língua detectada do comando em `OpenApplication`, `OpenUrl`, `OpenDocument` e `SearchWeb`; para documentos, usa o idioma alvo da leitura/tradução.

Japonês e coreano são aceitos como idiomas de comando/documento, mas você precisa instalar uma voz Piper compatível ou plugar outro provider TTS local para pronúncia natural nesses idiomas.

No GNOME, o fluxo principal de feedback visual é o indicador de barra `visionclip-status@visionclip`. Ele lê `~/.local/state/visionclip/status.json`, mostra o ícone de microfone com animação compacta enquanto o microfone grava ou quando `Key` é detectado, e troca para um ícone de stop durante a fala por TTS. Clicar no stop executa:

```bash
visionclip --stop-speaking
```

O overlay central antigo foi desativado. Mesmo que uma configuração antiga ainda tenha `voice.overlay_enabled = true`, o cliente ignora essa opção e usa apenas o indicador de barra.

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
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build --release --workspace --all-features
scripts/guard_no_secrets.sh
```

O mesmo conjunto básico roda no GitHub Actions em `.github/workflows/ci.yml`.

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
- Wake word contínua existe como opção local, mas ainda é um modo inicial; por padrão ela bloqueia ativação durante playback e ainda precisa evoluir para um detector dedicado com AEC/VAD streaming.
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
- OpenVoiceOS pt-BR Dii Piper voice: https://huggingface.co/OpenVoiceOS/pipertts_pt-BR_dii
- PipeWire echo-cancel: https://docs.pipewire.org/page_module_echo_cancel.html
- WebRTC Audio Processing Module: https://webrtc.googlesource.com/src/+/main/modules/audio_processing/g3doc/audio_processing_module.md
- Aether CSS Liquid Glass/Glassmorphism/Neumorphism reference: https://aethercss.lovable.app/

## Licença

Este projeto é distribuído sob a GNU Affero General Public License v3.0. Consulte [LICENSE](LICENSE) para o texto completo.

Se você executar o VisionClip como serviço acessível por rede e modificar o código, a AGPLv3 exige que você disponibilize o código-fonte correspondente dessas modificações aos usuários desse serviço.

## Contribuindo

Contribuições são bem-vindas. Priorize mudanças pequenas, testáveis e com contexto técnico claro.

Consulte também [CONTRIBUTING.md](CONTRIBUTING.md).

Em nome de R. Rodrigues.
