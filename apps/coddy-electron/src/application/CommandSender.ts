// application/CommandSender.ts
// Use case: sends user commands to the REPL backend.

import type {
  ModelRef,
  ModelRole,
  AssessmentPolicy,
  ReplIpcClient,
  ReplCommandResult,
  ReplMode,
  ScreenAssistMode,
} from '@/domain'

/**
 * Sends a text question to the REPL backend.
 * The result (text/summary/error) comes back after the daemon finishes.
 */
export async function sendAsk(
  client: ReplIpcClient,
  text: string,
): Promise<ReplCommandResult> {
  return client.ask(text)
}

/**
 * Sends a pre-transcribed voice turn.
 */
export async function sendVoiceTurn(
  client: ReplIpcClient,
  transcript: string,
): Promise<ReplCommandResult> {
  return client.voiceTurn(transcript)
}

/**
 * Requests the daemon to stop its current generation run.
 */
export async function cancelRun(client: ReplIpcClient): Promise<void> {
  await client.stopActiveRun()
}

/**
 * Requests the daemon to stop TTS playback immediately.
 */
export async function cancelSpeech(client: ReplIpcClient): Promise<void> {
  await client.stopSpeaking()
}

/**
 * Selects a backend model for the requested REPL role.
 */
export async function selectModel(
  client: ReplIpcClient,
  model: ModelRef,
  role: ModelRole,
): Promise<ReplCommandResult> {
  return client.selectModel(model, role)
}

/**
 * Switches the backend REPL UI mode. The reducer applies the emitted
 * OverlayShown event so all windows converge on the daemon state.
 */
export async function openUi(
  client: ReplIpcClient,
  mode: ReplMode,
): Promise<ReplCommandResult> {
  return client.openUi(mode)
}

/**
 * Captures voice through the platform-specific backend. In Electron this
 * already sends the transcribed VoiceTurn to the daemon, so callers must not
 * feed the returned text back into ask().
 */
export async function captureVoice(
  client: ReplIpcClient,
): Promise<ReplCommandResult> {
  return client.captureVoice()
}

/**
 * Requests a policy-aware screen assistance flow.
 */
export async function captureAndExplain(
  client: ReplIpcClient,
  mode: ScreenAssistMode,
  policy: AssessmentPolicy,
): Promise<ReplCommandResult> {
  return client.captureAndExplain(mode, policy)
}

/**
 * Dismisses a pending policy confirmation without routing text to the LLM.
 */
export async function dismissConfirmation(
  client: ReplIpcClient,
): Promise<ReplCommandResult> {
  return client.dismissConfirmation()
}
