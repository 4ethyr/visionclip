# Contrato Wire do Coddy

Este documento define o contrato mínimo que deve continuar estável quando o Coddy for movido para um repositório separado.

## Transporte Atual

O runtime atual usa Unix domain socket local com frames binários:

```text
u32 big-endian length
bincode payload
```

O framing é implementado em `crates/coddy-ipc` por:

- `write_frame`
- `read_frame`
- `write_frame_payload`
- `read_frame_payload`

Clientes Coddy não devem reimplementar o framing. A UI ou bridge deve chamar `coddy-client` ou reutilizar `coddy-ipc`.

## Envelope Direto

Todo payload nativo do Coddy deve usar:

```rust
CoddyWireRequest {
    magic: *b"CDDY",
    protocol_version: CODDY_PROTOCOL_VERSION,
    request: CoddyRequest,
}

CoddyWireResult {
    magic: *b"CDDY",
    protocol_version: CODDY_PROTOCOL_VERSION,
    result: CoddyResult,
}
```

Valores atuais:

```rust
CODDY_PROTOCOL_MAGIC = *b"CDDY"
CODDY_PROTOCOL_VERSION = 1
```

## Detecção de Protocolo

Servidores devem usar:

```rust
decode_wire_request_payload(payload)
```

Comportamento obrigatório:

- Se o payload decodificar como `CoddyWireRequest` e `magic == "CDDY"`, validar a versão e retornar `Some(CoddyRequest)`.
- Se o payload não for `CoddyWireRequest` ou tiver outro magic, retornar `Ok(None)` para permitir fallback legado.
- Se o magic for `CDDY`, mas a versão for incompatível, retornar erro estruturado.
- Se o magic for `CDDY`, mas houver bytes extras após o payload bincode, retornar erro estruturado em vez de aceitar truncamento/concatenação silenciosa.

Clientes ou bridges que precisem interpretar respostas cruas devem usar:

```rust
decode_wire_result_payload(payload)
```

## Compatibilidade Legada

O daemon VisionClip ainda aceita `VisionRequest`/`JobResult` para clientes antigos. Isso é compatibilidade do VisionClip, não contrato público do Coddy.

O Coddy deve falar apenas:

- `CoddyWireRequest`
- `CoddyWireResult`
- `CoddyRequest`
- `CoddyResult`

## Correlação de Requests

Todo `CoddyRequest` e todo `CoddyResult` expõe `request_id()`.

Regras:

- O cliente deve rejeitar respostas cujo `request_id` não corresponda ao request enviado.
- Frames de `CoddyRequest::EventStream` devem retornar `CoddyResult::ReplEvents` com o mesmo `request_id` da inscrição do stream.
- Erros top-level no daemon devem preservar o `request_id` original quando o request já tiver sido decodificado.

## Configuração Cross-Repo

Quando separado em outro repositório, o Coddy deve apontar para o daemon por:

```bash
CODDY_DAEMON_SOCKET=/run/user/$UID/visionclip/daemon.sock
```

Ordem de configuração do CLI:

1. `CODDY_CONFIG`
2. `VISIONCLIP_CONFIG`
3. `AI_SNAP_CONFIG`
4. Caminho padrão do VisionClip atual
5. Caminho legado do AI Snap

Isso permite que o Coddy rode com configuração própria sem quebrar usuários que já possuem `VISIONCLIP_CONFIG`.

## Testes Obrigatórios

Antes de publicar alterações incompatíveis:

```bash
cargo test -p coddy-ipc
cargo test -p coddy-client
cargo test -p visionclip-daemon coddy_wire_payload
```

Mudanças em enum variants, ordem de campos, magic, versão ou framing devem ser tratadas como mudança de protocolo e exigem atualização explícita de `CODDY_PROTOCOL_VERSION`.

Decodificadores devem rejeitar trailing bytes. Isso impede que mensagens concatenadas ou payloads parcialmente interpretados sejam aceitos como contrato válido.
