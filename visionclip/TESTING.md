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
- `Configured model probe: ok`
- `Portal screenshot backends:` deve listar pelo menos um backend quando você quiser usar captura automática via portal no Wayland

## Fluxo manual recomendado

1. Inicie o daemon:

```bash
visionclip-daemon
```

2. Exercite as ações principais com uma captura já existente:

```bash
visionclip --action translate_ptbr --image /caminho/captura.png
visionclip --action explain --image /caminho/captura.png
visionclip --action search_web --image /caminho/captura.png
visionclip --action copy_text --image /caminho/captura.png
visionclip --action extract_code --image /caminho/captura.png
```

3. Valide a captura nativa automática sem `--image`:

```bash
visionclip --action explain
```

4. Valide o caminho de captura por comando:

```bash
visionclip --action explain --capture-command 'maim -s -u'
```

5. Se o Piper HTTP estiver ativo, valide áudio:

```bash
visionclip --action explain --image /caminho/captura.png --speak
visionclip --action search_web --image /caminho/captura.png --speak
```

## Troubleshooting

- Se o Ollama retornar `does not support thinking`, o cliente faz retry automaticamente sem o campo `think`.
- Se o Ollama retornar `unable to load model`, o problema está no runtime ou no blob do modelo carregado pelo host.
- Se o Piper não estiver ativo, os fluxos de áudio não poderão ser validados fim a fim.
- Em Wayland, o launcher tenta portal com `gdbus` primeiro quando configurado; se isso falhar, ele depende de `gnome-screenshot` ou `grim`.
- Se o portal expirar, confira se o diálogo do `xdg-desktop-portal` foi concluído e se o `doctor` lista um backend de screenshot compatível para a sessão atual.
- Em GNOME Wayland, a API D-Bus direta de screenshot do Shell pode responder `AccessDenied` para clientes CLI; o caminho suportado pelo VisionClip continua sendo portal ou utilitários nativos instalados no host.
- Em X11, o launcher depende principalmente de `maim` ou `gnome-screenshot` para captura automática.
