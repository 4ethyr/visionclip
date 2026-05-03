# VisionClip Repository Boundaries

VisionClip and Coddy are now separated into sibling repositories during local development:

- VisionClip: `/home/aethyr/Documents/visionclip`
- Coddy: `/home/aethyr/Documents/coddy`

## VisionClip-owned code

VisionClip keeps the daemon, runtime, capture, inference, output, TTS, configuration, scripts, examples and deployment assets:

- `apps/visionclip`
- `apps/visionclip-daemon`
- `apps/visionclip-config`
- `crates/common`
- `crates/infer`
- `crates/output`
- `crates/tts`

## Coddy-owned code

Coddy-owned apps, crates, UI prototypes, REPL docs and agent prompt pack must stay in the Coddy repository:

- `apps/coddy`
- `apps/coddy-electron`
- `crates/coddy-client`
- `crates/coddy-core`
- `crates/coddy-ipc`
- `crates/coddy-voice-input`
- `docs/repl`
- `repl_ui`
- `.agent`
- `AGENTS.md`

## Temporary protocol compatibility

The VisionClip daemon still implements the Coddy protocol behind the explicit `coddy-protocol` feature, which is disabled by default so VisionClip can build as a standalone repository. VisionClip must not depend on Coddy crates by path, workspace, git or registry dependency. The temporary compatibility layer lives in `apps/visionclip-daemon/src/coddy_contract.rs` and must only mirror the wire types needed by the daemon bridge, including the read-only REPL tools catalog while Coddy is being split out.

Within the daemon source, Coddy protocol handling is intentionally confined to `apps/visionclip-daemon/src/coddy_bridge.rs` and `apps/visionclip-daemon/src/coddy_contract.rs`. Native VisionClip frame reading and payload decoding live in `visionclip-common`, so the daemon entrypoint does not need to import Coddy IPC utilities for VisionClip requests.

`coddy_bridge.rs` also owns the Coddy REPL runtime state, event stream, event construction, command dispatch, Ask/VoiceTurn orchestration, intent event mapping, voice-turn intent adaptation, local REPL commands and policy-only screen-assist lifecycle. The daemon entrypoint no longer owns the Coddy turn pipeline and must stay limited to adapting native VisionClip services such as inference, search, TTS, app launching and URL opening through `ReplNativeServices`. This keeps the Coddy protocol surface behind the bridge while allowing the native VisionClip runtime to remain private to the daemon.

`visionclip-common` and the `visionclip` CLI must stay free of Coddy crates. Their IPC surface is limited to native VisionClip operations: capture, voice search, open application, open URL, open local document and healthcheck.

This is an integration boundary, not ownership of the Coddy project. The Coddy app, Electron UI, REPL docs and prompt pack should not be reintroduced into the VisionClip workspace.

## Guardrail

`crates/common/tests/repository_boundaries.rs` fails if Coddy-owned directories return to the VisionClip repository, if Coddy packages are added back as VisionClip workspace members, if any VisionClip manifest declares Coddy crate dependencies, if the `coddy-protocol` feature starts enabling sibling path dependencies, if `visionclip-common`/`visionclip` start depending on Coddy crates again, if `coddy_core` or `coddy_ipc` usage appears in VisionClip sources, if the bridge stops owning the Coddy REPL command pipeline, if `main.rs` stops being a native-services adapter, if `apps/visionclip-daemon/src/main.rs` starts importing Coddy core/protocol details directly, if it starts constructing `ReplEvent`/`ReplIntent` values directly again, or if the daemon entrypoint starts calling Coddy voice intent internals directly again.
