# Coddy Technical Assistant e Assessment Mode

## Objetivo

O VisionClip deve ajudar o usuário a entender problemas técnicos, interpretar telas, explicar código, depurar erros e praticar desafios. Esse recurso deve ser útil para:

- estudos;
- coding practice;
- revisão de algoritmos;
- debug de IDE;
- explicação de testes;
- entendimento de múltipla escolha;
- entrevistas simuladas;
- assessments onde o uso de IA é explicitamente permitido.

Também deve evitar uso indevido em avaliações ativas quando a plataforma ou recrutador proíbe assistência externa.

## Política de Integridade

### Estados de Política

```rust
pub enum AssessmentPolicy {
    Practice,
    PermittedAi,
    SyntaxOnly,
    RestrictedAssessment,
    UnknownAssessment,
}
```

### Comportamento por Estado

| Estado | Pode resposta direta? | Pode código completo? | Pode explicar conceito? | Pode dar dica? |
| --- | --- | --- | --- | --- |
| `Practice` | Sim | Sim | Sim | Sim |
| `PermittedAi` | Sim | Sim, com transparência | Sim | Sim |
| `SyntaxOnly` | Não para lógica | Não | Sim, sintaxe/documentação | Sim, limitado |
| `RestrictedAssessment` | Não | Não | Sim | Sim, sem resposta final |
| `UnknownAssessment` | Pedir confirmação | Não até confirmar | Sim | Sim, cauteloso |

### Regra Central

Se a tela aparentar ser uma avaliação ativa e a política não indicar que IA é permitida, o assistente deve:

- não escolher alternativa final;
- não escrever solução completa;
- não dar código pronto para submissão;
- não sugerir burlar proctoring, fullscreen, detecção de aba ou screenshot;
- oferecer explicação de conceitos, decomposição do problema, dicas e revisão de raciocínio.

## Detecção de Contexto de Assessment

Sinais visuais/textuais:

- domínios ou logos: HackerRank, CodeSignal, Coderbyte, LeetCode Assessments, Codility, TestGorilla, BairesDev, DevSkiller;
- palavras: `assessment`, `proctored`, `timer`, `submit`, `question`, `multiple choice`, `certified`, `rules`;
- cronômetro ativo;
- layout de pergunta + opções;
- editor online + botão submit;
- avisos de tela cheia/proctoring.

```rust
pub struct AssessmentSignal {
    pub kind: AssessmentSignalKind,
    pub value: String,
    pub confidence: f32,
}
```

## Fluxo de Múltipla Escolha

### Entrada

Screenshot com pergunta e alternativas.

### Pipeline

1. Capturar tela.
2. OCR.
3. Detectar `QuestionBlock`.
4. Extrair alternativas.
5. Detectar política.
6. Se permitido, resolver e explicar.
7. Se restrito/desconhecido, explicar conceitos e orientar raciocínio sem marcar alternativa final.

### Estrutura

```rust
pub struct MultipleChoiceContext {
    pub question: String,
    pub options: Vec<ChoiceOption>,
    pub topic: Option<String>,
    pub language: Option<String>,
    pub source_platform: Option<String>,
    pub policy: AssessmentPolicy,
}

pub struct ChoiceOption {
    pub label: String,
    pub text: String,
    pub selected: bool,
}
```

### Resposta em treino/permissão

```text
A opção correta parece ser B.
Motivo: ...
As alternativas A e C confundem ...
```

### Resposta em assessment restrito

```text
Parece ser uma avaliação ativa. Posso ajudar a interpretar a pergunta e eliminar alternativas por conceito, mas não vou indicar a resposta final. A pergunta está testando ...
```

## Fluxo de Código em IDE ou Assessment

### Entrada

Screenshot de IDE, terminal, editor online ou erro de compilação.

### Pipeline

1. Capturar tela.
2. OCR e segmentação.
3. Detectar linguagem.
4. Extrair blocos de código.
5. Usar Tree-sitter quando possível.
6. Extrair erro/stack trace/test output.
7. Construir contexto.
8. Aplicar política.
9. Responder.

```rust
pub struct CodeAssistContext {
    pub language: Option<String>,
    pub code_blocks: Vec<CodeBlock>,
    pub visible_error: Option<String>,
    pub tests_visible: Vec<String>,
    pub editor_context: Option<String>,
    pub platform: Option<String>,
    pub policy: AssessmentPolicy,
}
```

### Modos de Resposta

#### `Explain`

Explica o que o código faz, riscos e bugs prováveis.

#### `Debug`

Identifica causa provável do erro e sugere investigação.

#### `Guide`

Ajuda com abordagem, sem entregar solução final.

#### `Patch`

Permitido apenas fora de assessment restrito ou quando IA é permitida.

#### `Test`

Gera casos de teste para validar raciocínio, com cuidado para não substituir solução do candidato quando restrito.

## Prompts Internos

### Classificador de Tela

```text
Você classifica uma captura de tela para o VisionClip.

Retorne JSON válido:
{
  "screen_kind": "ide|terminal|browser_search|assessment_multiple_choice|assessment_code|documentation|unknown",
  "platform": "string|null",
  "assessment_signals": [{"kind":"string","value":"string","confidence":0.0}],
  "contains_code": true,
  "contains_question": true,
  "contains_choices": true,
  "language_hint": "string|null",
  "confidence": 0.0
}

Não resolva a pergunta. Apenas classifique e extraia sinais.
```

### Extrator de Múltipla Escolha

```text
Extraia a pergunta e as alternativas da captura.

Retorne JSON válido:
{
  "question": "texto da pergunta",
  "options": [
    {"label":"A","text":"...","selected":false}
  ],
  "topic": "assunto provável",
  "language": "pt-BR|en|unknown",
  "confidence": 0.0
}

Não responda qual alternativa está correta.
Preserve símbolos técnicos importantes, mas normalize ruído visual.
```

### Resolvedor Permitido

```text
Você ajuda em treino ou avaliação onde IA é permitida.

Use apenas o contexto fornecido.
Se for múltipla escolha, indique a melhor opção e explique por que ela é correta.
Se houver ambiguidade, diga qual informação falta.
Não invente enunciado, alternativa ou código que não esteja no contexto.

Formato:
Resposta curta:
Explicação:
Riscos/ambiguidade:
```

### Guia Restrito

```text
Você ajuda em uma avaliação ativa onde assistência externa pode ser restrita.

Não forneça a resposta final.
Não escolha alternativa.
Não escreva solução completa.
Ajude o usuário a entender o conceito, decompor o problema e verificar seu próprio raciocínio.

Formato:
O que a pergunta está testando:
Como pensar:
Pistas para eliminar alternativas:
Próximo passo:
```

### Debug de Código

```text
Você analisa código visível em uma captura de tela.

Tarefas:
1. Identifique linguagem e estrutura.
2. Aponte erro provável ou risco.
3. Explique o raciocínio de forma didática.
4. Se a política permitir, sugira patch ou pseudocódigo.
5. Se a política restringir, dê dicas e testes conceituais, sem solução pronta.

Não invente arquivos ausentes.
Quando OCR parecer incerto, indique a incerteza.
```

## UX de Política

O REPL deve mostrar um badge:

- `Practice`: cyan.
- `AI Allowed`: cyan + violet.
- `Syntax Only`: amarelo discreto.
- `Restricted`: vermelho desaturado.
- `Unknown`: cinza com botão "Definir modo".

Quando `UnknownAssessment`, perguntar:

```text
Esta é uma avaliação ativa com uso de IA permitido?
```

Opções:

- `Treino / prática`
- `IA permitida`
- `Apenas sintaxe`
- `Não sei / restrito`

## Exemplos

### Prática de múltipla escolha

Usuário: print de pergunta sobre complexidade de busca binária.

Resposta:

```text
Resposta curta: O(log n).

Explicação: a busca binária reduz o espaço de busca pela metade a cada passo...
```

### Assessment desconhecido

Resposta:

```text
Parece uma tela de assessment. Antes de eu indicar uma resposta, confirme se o uso de IA é permitido. Enquanto isso, posso explicar o conceito: a pergunta parece testar complexidade assintótica...
```

### Código em IDE local

Resposta:

```text
O erro provável está no await ausente em fetchUser(). A função retorna Promise<User>, mas o código está tentando acessar propriedades como se fosse User.
```

### Código em assessment restrito

Resposta:

```text
Não vou fornecer a solução completa. A abordagem é: modele o problema como uma janela deslizante, mantenha dois ponteiros e atualize o melhor resultado quando a restrição for satisfeita.
```

## Critérios de Aceite

- Detecta múltipla escolha em screenshots com pelo menos 85% de precisão em conjunto local de teste.
- Extrai pergunta e opções com confiança separada.
- Nunca dá resposta final quando política é `RestrictedAssessment`.
- Permite resposta direta em `Practice` e `PermittedAi`.
- Registra no histórico qual política foi aplicada.
- Todas as respostas citam incerteza quando OCR for baixa confiança.
- Testes unitários cobrem policy gating antes de prompts.
