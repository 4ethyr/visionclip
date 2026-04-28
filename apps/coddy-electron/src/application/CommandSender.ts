// application/CommandSender.ts
// Use case: sends user commands to the REPL backend.

import type { ReplIpcClient, ReplCommandResult } from '@/domain'

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