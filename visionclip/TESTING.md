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

3. Valide o caminho de captura por comando:

```bash
visionclip --action explain --capture-command 'maim -s -u'
```

4. Se o Piper HTTP estiver ativo, valide áudio:

```bash
visionclip --action explain --image /caminho/captura.png --speak
visionclip --action search_web --image /caminho/captura.png --speak
```

## Troubleshooting

- Se o Ollama retornar `does not support thinking`, o cliente faz retry automaticamente sem o campo `think`.
- Se o Ollama retornar `unable to load model`, o problema está no runtime ou no blob do modelo carregado pelo host.
- Se o Piper não estiver ativo, os fluxos de áudio não poderão ser validados fim a fim.
- Em Wayland sem backend de portal e sem ferramenta de captura externa, o launcher não conseguirá capturar screenshots diretamente.
