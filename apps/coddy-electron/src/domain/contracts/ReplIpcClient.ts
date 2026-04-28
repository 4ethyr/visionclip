// domain/contracts/ReplIpcClient.ts
// Port interface — implemented by infrastructure

import type {
  ModelRef,
  ModelRole,
  ReplEventEnvelope,
  ReplMode,
  ReplSessionSnapshot,
  ScreenAssistMode,
  AssessmentPolicy,
} from '../types'

/** Result of sending a command to the REPL backend */
export interface ReplCommandResult {
  text?: string
  summary?: string
  message?: string
  error?: { code: string; message: string }
}

/** Batch of incremental events */
export interface ReplEventsBatch {
  events: ReplEventEnvelope[]
  lastSequence: number
}

/**
 * Abstraction over the Coddy REPL backend transport.
 *
 * The frontend never knows whether it's talking to:
 * - a spawned `coddy` CLI child process
 * - a Unix socket via Tauri command
 * - an HTTP bridge
 *
 * It just calls these methods and gets typed results.
 */
export interface ReplIpcClient {
  /** Get a full session snapshot (state + last sequence) */
  getSnapshot(): Promise<ReplSessionSnapshot>

  /** Get incremental events after a given sequence number */
  getEventsAfter(afterSequence: number): Promise<ReplEventsBatch>

  /** Open a persistent stream of live events. Returns an AsyncIterable. */
  watchEvents(afterSequence: number): AsyncIterable<ReplEventEnvelope>

  /** Send an `ask` text command */
  ask(text: string): Promise<ReplCommandResult>

  /** Send a `voice turn` command (with pre-transcribed text) */
  voiceTurn(transcript: string): Promise<ReplCommandResult>

  /** Stop the active assistant run (cancel generation) */
  stopActiveRun(): Promise<void>

  /** Stop TTS speech immediately */
  stopSpeaking(): Promise<void>

  /** Select a model for a specific REPL role */
  selectModel(model: ModelRef, role: ModelRole): Promise<ReplCommandResult>

  /** Ask the backend to open/switch the REPL UI mode */
  openUi(mode: ReplMode): Promise<ReplCommandResult>

  /** Request a policy-aware screen assist run */
  captureAndExplain(
    mode: ScreenAssistMode,
    policy: AssessmentPolicy,
  ): Promise<ReplCommandResult>

  /** Dismiss a pending policy confirmation without sending prompt text */
  dismissConfirmation(): Promise<ReplCommandResult>

  /**
   * Capture voice via the system mic (spawns `coddy voice --overlay`).
   * The CLI handles recording, STT, and sends VoiceTurn to the daemon.
   * Returns the text result or an error.
   */
  captureVoice(): Promise<ReplCommandResult>
}
