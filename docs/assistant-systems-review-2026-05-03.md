# Revisão dos sistemas do assistente - 2026-05-03

Esta revisão cobre RAG, inferência, roteamento, OCR, multilíngue, comandos,
tradução, leitura de documentos e contexto. O objetivo é comparar o VisionClip
atual com práticas de mercado e apontar próximos incrementos seguros.

## Fontes pesquisadas

- Microsoft Azure Architecture Center, RAG design/evaluation:
  https://learn.microsoft.com/fil-ph/azure/architecture/ai-ml/guide/rag/rag-solution-design-and-evaluation-guide
- Microsoft Azure Architecture Center, retrieval, hybrid search, RRF, reranking:
  https://learn.microsoft.com/en-us/azure/architecture/ai-ml/guide/rag/rag-information-retrieval
- OWASP GenAI / LLM Top 10:
  https://owasp.org/www-project-top-10-for-large-language-model-applications/
- OWASP MCP Top 10:
  https://owasp.org/www-project-mcp-top-10/
- LangChain RAG docs, indirect prompt injection in retrieved context:
  https://docs.langchain.com/oss/python/langchain/rag
- OpenAI function calling / Structured Outputs:
  https://help.openai.com/en/articles/8555517-function-calling-in-the-openai-api
- OpenAI Whisper:
  https://openai.com/index/whisper/
- faster-whisper:
  https://github.com/SYSTRAN/faster-whisper
- Azure Document Intelligence layout model:
  https://learn.microsoft.com/en-us/azure/ai-services/document-intelligence/prebuilt/layout
- Piper:
  https://github.com/rhasspy/piper
- Ollama Gemma 4:
  https://ollama.com/library/gemma4
- Hugging Face Gemma 4 E2B:
  https://huggingface.co/google/gemma-4-E2B-it

## Estado atual detectado

- Runtime central em Rust com CLI -> Unix socket -> daemon.
- `crates/common` concentra IPC, configuração, linguagem, ações, sessão,
  auditoria, segurança, permission engine e tool registry.
- `crates/infer` já possui `AiProvider`, `ProviderRouter`, capabilities e
  requests para chat, visão, OCR, embeddings, tradução e busca.
- `apps/visionclip-daemon` executa roteamento local-first, OCR/captura,
  pesquisa renderizada, ações Linux, RAG de documentos, TTS e auditoria.
- `crates/documents` tem ingestão local de TXT/Markdown/PDF, chunking,
  SQLite, progresso de leitura, tradução incremental, cache de tradução e cache
  de áudio.
- `apps/visionclip/src/voice.rs` faz captura/transcrição one-shot por comando
  configurável, detecta idioma do transcript e envia metadados ao daemon para
  abertura de apps, URLs, documentos e pesquisa.
- `apps/visionclip-daemon/src/local_files.rs` busca documentos locais por nome
  com normalização, tokens multilíngues de comando e aliases para erros comuns
  de STT, incluindo títulos em inglês dentro de comandos em português/espanhol.

## Comparação com padrões de mercado

### RAG e documentos

O VisionClip já cobre o essencial de RAG local-first: ingestão, chunks,
embeddings opcionais, fallback lexical e prompt grounded. A literatura atual
recomenda evoluir para busca híbrida, avaliação de chunking, metadados por
chunk, reranking e métricas de recuperação. O código ainda usa cosine puro para
embeddings e contagem lexical simples como fallback; não há RRF, BM25, MMR,
cross-encoder, citações estruturadas nem eval set de perguntas/documentos.

Prioridade:

1. Adicionar metadados de citação por chunk: página, seção, caminho e score.
2. Combinar lexical + embeddings por Reciprocal Rank Fusion antes do limite de
   contexto.
3. Aplicar MMR ou deduplicação por diversidade para evitar chunks repetidos.
4. Criar evals locais com perguntas esperadas, chunks relevantes e groundedness.
5. Adicionar OCR local para PDFs escaneados com consentimento explícito.

### Segurança de RAG e tool use

OWASP classifica prompt injection, vazamento de dados, agência excessiva e
fraquezas de embeddings/RAG como riscos centrais. O VisionClip já tem boa base:
tool registry, schemas, risk levels, permission engine, auditoria e política
local-only para dados sensíveis. A lacuna mais importante era o prompt de RAG
não separar explicitamente contexto recuperado de instruções. Esta revisão
reforçou os prompts de documentos para tratar trechos como dados não confiáveis.

Próximo passo: todos os prompts que recebem OCR, documentos, busca web ou
ferramentas externas devem incluir a mesma fronteira de dados não confiáveis e
testes contra injeções indiretas.

### Inferência e roteamento

A arquitetura `AiProvider`/`ProviderRouter` segue o padrão correto: capability
routing, local-first e bloqueio de cloud em conteúdo sensível. O próximo nível é
roteamento por saúde, latência e qualidade, com circuit breaker por provider e
tracing de seleção. Cloud providers devem continuar opt-in e nunca receber OCR,
terminal, documentos privados ou screenshots sem confirmação.

Próximo passo:

1. Persistir `provider.selected`, latência, erro e fallback em eventos
   consultáveis.
2. Implementar structured outputs apenas como conveniência; validação local de
   schema continua sendo a fronteira de segurança.
3. Promover STT/TTS para providers do router sem acoplar o core ao
   faster-whisper ou Piper.

### OCR e contexto de tela

O fluxo atual executa OCR dedicado quando configurado e cai para visão
multimodal. Isso atende o MVP, mas ainda é texto plano. Sistemas robustos de
document/screen understanding preservam layout: página, linha, palavra,
bounding boxes, confiança, ordem natural, tabelas e regiões. Isso é necessário
para tradução visual, leitura de tela, explicação de erro e automação futura.

Próximo passo:

1. Criar `ScreenContext` com regiões `{bbox, text, confidence, kind}`.
2. Manter OCR texto simples como compatibilidade.
3. Salvar método de extração e confiança na auditoria.
4. Para Wayland, continuar priorizando XDG Desktop Portal e consentimento.

### Voz e multilíngue

Whisper/faster-whisper são escolhas adequadas para STT local multilíngue,
especialmente com language detection, timestamps e VAD/streaming. O VisionClip
já propaga `AssistantLanguage` nos comandos de voz e seleciona vozes por idioma.
A fragilidade atual é normal: comandos curtos e nomes de livros em outro idioma
sofrem com ASR e detecção heurística.

Próximo passo:

1. Guardar `detected_language`, `asr_language_probability`, `duration_ms` e
   modelo STT nos logs.
2. Usar n-best/candidates quando o transcritor oferecer.
3. Criar corretor pós-ASR por domínio: apps instalados, sites conhecidos e
   índice de documentos locais.
4. Permitir hint de idioma por sessão, mas manter `auto` como padrão.
5. Para code-switching, separar intenção do comando e entidade preservada
   ("abra o livro" em português + título em inglês).

### Tradução e leitura de livros

O pipeline incremental com canais pequenos, cache de tradução, cache de áudio e
progresso já está alinhado ao objetivo de audiobook traduzido sem pré-processar
o livro inteiro. Ainda faltam controles mais ricos de áudio e QA contextual
durante a leitura.

Próximo passo:

1. `AudioRuntime` com comandos pause/resume/stop/skip independentes do player.
2. Pergunta contextual durante leitura: pausar, coletar janela de chunks lidos,
   responder no idioma atual e retomar.
3. Cache versionado por `{document_id, chunk_id, target_language, provider,
   model, voice_id, text_hash}`.
4. Suporte a EPUB real na ingestão; hoje a busca local encontra EPUB, mas a
   ingestão do runtime só suporta TXT/Markdown/PDF.

### Pesquisa

O VisionClip já faz busca, renderização e resposta grounded em idioma detectado.
O próximo passo é formalizar `SearchProvider`, retornar fontes estruturadas e
adicionar trust scoring simples por domínio, data e tipo de fonte.

Prioridade: toda pergunta temporal, versão, preço, notícia ou status deve
marcar busca como obrigatória; perguntas derivadas de terminal/código/documento
privado devem pedir confirmação antes de enviar query externa.

### Comandos e desktop Linux

A base de ações seguras está correta: comandos fixos, validação de URL/app,
risco e auditoria. Falta separar todas as operações Linux em um
`DesktopController` com executor mockável e confirmação real por IPC/UI para
risco >= 3.

## Mudança implementada nesta revisão

- `document ask` agora detecta o idioma da pergunta, passa `response_language`
  ao provider, usa voz TTS compatível e registra o idioma em auditoria.
- Prompts de pergunta e resumo de documentos foram parametrizados por idioma.
- Prompts de RAG agora instruem o modelo a tratar trechos recuperados como dados
  não confiáveis e a não seguir comandos embutidos nesses trechos.
- Foram adicionados testes unitários para garantir idioma parametrizado e guarda
  de grounding.

## Próximas melhorias recomendadas

1. RAG híbrido com RRF: unir lexical e embeddings antes de selecionar contexto.
2. Citações estruturadas em `JobResult` para respostas de documentos.
3. `DocumentAskJob`/`DocumentSummarizeJob` com `input_language` opcional em uma
   evolução compatível do IPC.
4. OCR layout-aware com regiões, confiança e bboxes.
5. Pós-processador ASR guiado por índice local de apps/sites/documentos.
6. Eval set multilíngue: comandos de voz curtos, comandos code-switched,
   perguntas RAG e OCR de tela.
7. AudioRuntime controlável para pausar/retomar leitura traduzida com baixa
   latência.
