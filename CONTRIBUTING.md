# Contribuindo com o VisionClip

Obrigado por considerar uma contribuição para o VisionClip.

## Prioridades atuais

- backend de captura Wayland via portal
- melhorias de OCR e extração de código
- integração real com Piper HTTP em diferentes distros
- robustez do daemon, observabilidade e diagnósticos
- testes fim a fim para fluxos de screenshot, inferência e áudio

## Como contribuir

1. Abra uma issue descrevendo o problema, risco ou melhoria.
2. Mantenha cada PR focado em uma mudança clara.
3. Inclua contexto técnico suficiente para revisão rápida.
4. Atualize testes e documentação quando o comportamento mudar.

## Padrões do projeto

- Rust 2021
- mudanças pequenas e verificáveis
- nomes e comandos públicos alinhados à marca `VisionClip`
- integração local-first com Linux, Ollama e Piper

## Checklist antes do PR

```bash
cargo fmt --all
cargo test --workspace
cargo build --workspace
```

Se você alterar fluxos do host, inclua também observações de validação manual.

## Licença

Ao contribuir, você concorda que sua contribuição seja distribuída sob a GNU AGPLv3, conforme o arquivo [LICENSE](LICENSE).
