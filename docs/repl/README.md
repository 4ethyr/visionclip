# Coddy REPL Assistant

Este diretório documenta o **Coddy**, a próxima evolução interativa do VisionClip: um REPL visual e conversacional para assistência técnica, análise de screenshots, contexto de código, voz e execução segura.

## Nome e Escopo

`Coddy` é o nome oficial da experiência REPL/CLI do VisionClip. O VisionClip permanece como plataforma local, daemon, captura, OCR, TTS e integrações Linux; o Coddy será a interface orientada a conversação, coding assistance e sessões agentic.

Convenção proposta:

- Binário do REPL: `coddy`
- App desktop: `apps/coddy`
- Core de domínio: `crates/coddy-core`
- Contratos IPC: `crates/coddy-ipc`
- Cliente de transporte: `crates/coddy-client`
- Frontend TypeScript: `apps/coddy/src`
- Nome de produto na UI: `Coddy`
- Referências antigas como `aether_cli`, `Aether Terminal` ou `AETHER_CORE`: tratar como protótipos/placeholder visual, não como marca final.

Política de migração:

- Novos módulos, pacotes, comandos, arquivos de configuração e labels visíveis devem usar `coddy`.
- O termo `Aether` só pode aparecer ao citar artefatos legados dentro de `repl_ui`.
- Antes de qualquer PR de implementação, rodar uma busca por `aether_cli`, `AETHER_`, `Aether` e `SYSTEM_REPL` para garantir que não viraram API pública.
- Se algum protótipo visual for convertido para código, renomear classes, IDs, tokens e textos de UI para Coddy no mesmo commit da conversão.

## Objetivo

O REPL deve permitir que o usuário interaja com a IA de duas formas:

- **Modo simples:** terminal flutuante, transparente, com blur, opacidade configurável, entrada por texto/voz e respostas em estilo agent CLI.
- **Modo advanced:** desktop app com painéis, workspace de contexto, seletor de modelos, histórico, execução agentic e gerenciamento de modelos/daemons locais.

O foco é auxiliar dúvidas técnicas, estudo, depuração, explicação de código, interpretação de telas e preparação/prática de assessments. Em provas, entrevistas ou testes de terceiros, o sistema deve respeitar as regras da plataforma e operar com guardrails explícitos.

## Documentos

- [Arquitetura](architecture.md): módulos Rust/TypeScript, IPC, eventos, estados do REPL, integração com daemon, voz, imagem e modelos locais.
- [Contratos do Backend](backend-contracts.md): implementação atual do backend Coddy, comandos CLI, IPC, snapshots, eventos incrementais e integração prevista com o frontend.
- [Contrato Wire](coddy-wire-contract.md): framing bincode, magic `CDDY`, versionamento e regras de compatibilidade cross-repo.
- [Coddy Client](coddy-client.md): adapter de transporte para CLI/UI consumir o daemon via snapshot, eventos, stream e comandos.
- [Próximos Passos do Backend](backend-next-steps.md): fases técnicas para stream, `coddy-ipc`, cliente, persistência, UI e separação de repositório.
- [Plano de Desacoplamento](coddy-decoupling-plan.md): caminho para mover Coddy para um repositório próprio mantendo integração estável com VisionClip.
- [OpenAPI/Swagger](openapi/README.md): especificação HTTP proposta para bridge Tauri/Swagger/OpenAI Actions baseada nos contratos Rust atuais.
- [UX/UI](ui-ux-spec.md): análise dos protótipos em `repl_ui`, adaptação da identidade para Coddy, tokens de design, modos simples/advanced, animações e comportamento responsivo.
- [Assistente de Assessments](assessment-assistant.md): interpretação de screenshots, múltipla escolha, código, política de integridade e prompts internos.
- [Plano de Implementação](implementation-plan.md): fases TDD, entregáveis, milestones, riscos e critérios de aceite.
- [Qualidade e Testes](quality-plan.md): estratégia de testes, mocks, métricas, acessibilidade, performance e observabilidade.
- [Pesquisa e Referências](research-notes.md): síntese das pesquisas externas usadas nesta especificação.

## Decisão de Produto

O Coddy não deve ser apenas uma tela bonita sobre o daemon atual. Ele deve ser uma camada de orquestração com estado próprio:

- Captura contexto de tela/código.
- Extrai contexto de buscas e regiões visíveis, incluindo Visão Geral por IA quando renderizada ao usuário.
- Classifica intenção.
- Envia tarefas ao daemon/LLM.
- Exibe streaming parcial.
- Permite interromper, pausar, reenviar e auditar.
- Respeita permissões e integridade de assessments.
- Mantém baixa latência, com degradação graciosa para modelos locais menores.

## Primeiro Marco

O primeiro marco de desenvolvimento deve ser o MVP de voz:

- atalho global confiável no GNOME/Kali;
- overlay transparente em estado `listening`;
- transcript real ou mockado;
- router com resposta textual;
- TTS serializado sem sobreposição;
- diagnóstico `coddy doctor shortcuts`;
- busca com contexto mockado antes de depender de Google real.

Esse marco valida a experiência central antes do modo desktop app avançado.

## Princípios

- **Local-first:** dados sensíveis, screenshots, áudio e contexto de código devem permanecer locais sempre que possível.
- **TDD:** cada intent, parser, reducer de estado, componente e fluxo de IPC deve nascer com testes.
- **Clean Architecture:** domínio de intents e sessões desacoplado da UI, do Tauri e dos provedores LLM/TTS/STT.
- **SOLID:** ações e provedores extensíveis via interfaces pequenas e testáveis.
- **Baixa latência:** streaming, cancelamento, cache de contexto e fila de áudio são requisitos de primeira classe.
- **Integridade:** o assistente não deve burlar regras de plataformas de teste nem fornecer respostas diretas quando o uso de IA não for permitido.
