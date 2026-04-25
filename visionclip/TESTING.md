# Validação do VisionClip

## Testes automatizados

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace --release
```

## Preparação do host

1. Liste os modelos disponíveis no runtime local:

```bash
visionclip-config models
```

2. Rode o diagnóstico completo do host:

```bash
visionclip-config doctor
```

3. Confirme antes do teste fim a fim:

- `Configured model available: yes`
- `Configured OCR model available: yes`
- `Configured model probe: ok`
- `Configured OCR model probe: ok`
- `Portal screenshot backends:` deve listar pelo menos um backend quando você quiser usar captura automática via portal no Wayland

## Fluxo manual recomendado

1. Inicie o daemon:

```bash
VISIONCLIP_CONFIG=/home/aethyr/Documents/visionclip/visionclip/tools/visionclip-hybrid.toml visionclip-daemon
```

2. Exercite as ações principais com uma captura já existente:

```bash
VISIONCLIP_CONFIG=/home/aethyr/Documents/visionclip/visionclip/tools/visionclip-hybrid.toml visionclip --action translate_ptbr --image /caminho/captura.png
VISIONCLIP_CONFIG=/home/aethyr/Documents/visionclip/visionclip/tools/visionclip-hybrid.toml visionclip --action explain --image /caminho/captura.png
VISIONCLIP_CONFIG=/home/aethyr/Documents/visionclip/visionclip/tools/visionclip-hybrid.toml visionclip --action search_web --image /caminho/captura.png
VISIONCLIP_CONFIG=/home/aethyr/Documents/visionclip/visionclip/tools/visionclip-hybrid.toml visionclip --action copy_text --image /caminho/captura.png
VISIONCLIP_CONFIG=/home/aethyr/Documents/visionclip/visionclip/tools/visionclip-hybrid.toml visionclip --action extract_code --image /caminho/captura.png
```

Em `search_web`, valide tambem se:

- a query gerada faz sentido para o contexto da captura
- o launcher imprime um resumo inicial quando o scrape de busca retorna dados uteis
- o clipboard recebe esse resumo quando houver enriquecimento disponivel

3. Valide a captura nativa automática sem `--image`:

```bash
VISIONCLIP_CONFIG=/home/aethyr/Documents/visionclip/visionclip/tools/visionclip-hybrid.toml visionclip --action explain
```

4. Valide o caminho de captura por comando:

```bash
VISIONCLIP_CONFIG=/home/aethyr/Documents/visionclip/visionclip/tools/visionclip-hybrid.toml visionclip --action explain --capture-command 'maim -s -u'
```

5. Se o Piper HTTP estiver ativo, valide áudio:

```bash
VISIONCLIP_CONFIG=/home/aethyr/Documents/visionclip/visionclip/tools/visionclip-hybrid.toml visionclip --action explain --image /caminho/captura.png --speak
VISIONCLIP_CONFIG=/home/aethyr/Documents/visionclip/visionclip/tools/visionclip-hybrid.toml visionclip --action search_web --image /caminho/captura.png --speak
```

## Scripts de apoio

Subir stack local completa:

```bash
./scripts/start_local_stack.sh
```

Rodar Explicar, Traduzir e Pesquisar com TTS:

```bash
# Usa captura automatica
./scripts/test_tts_flows.sh

# Usa a mesma imagem para os tres fluxos
./scripts/test_tts_flows.sh --image /caminho/captura.png
```

Encerrar os processos iniciados pelos helpers:

```bash
./scripts/stop_local_stack.sh
```

## Troubleshooting

- Se o Ollama retornar `does not support thinking`, o cliente faz retry automaticamente sem o campo `think`.
- Se o modelo principal retornar uma resposta muito curta ou vaga em imagem direta, valide se a config ativa aponta para `ocr_model = "gemma4:e2b"`; o default atual foi ajustado para `OCR -> Gemma -> Gemma`.
- Se o Ollama retornar `unable to load model`, o problema está no runtime ou no blob do modelo carregado pelo host.
- Se o Piper não estiver ativo, os fluxos de áudio não poderão ser validados fim a fim.
- O scrape de busca e best-effort; se o Google nao responder a tempo ou nao expor HTML util, o VisionClip continua abrindo a consulta no navegador sem resumo enriquecido.
- Em Wayland, o launcher tenta portal com `gdbus` primeiro quando configurado; se isso falhar, ele depende de `gnome-screenshot` ou `grim`.
- Se o portal expirar, confira se o diálogo do `xdg-desktop-portal` foi concluído e se o `doctor` lista um backend de screenshot compatível para a sessão atual.
- Em GNOME Wayland, a API D-Bus direta de screenshot do Shell pode responder `AccessDenied` para clientes CLI; o caminho suportado pelo VisionClip continua sendo portal ou utilitários nativos instalados no host.
- Em X11, o launcher depende principalmente de `maim` ou `gnome-screenshot` para captura automática.
