// application/EventStreamer.ts
// Use case: manages a live event stream from the backend with reconnection.

import type { ReplIpcClient, ReplEventEnvelope } from '@/domain'
import type { SessionState } from './SessionManager'
import { applyEvents } from './SessionManager'

export type StreamCallback = (state: SessionState) => void
export type ErrorCallback = (error: Error) => void

const BASE_DELAY_MS = 500
const MAX_DELAY_MS = 10_000

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

/**
 * Opens a live watch stream and continuously applies events
 * to the session state, notifying via callback on each batch.
 *
 * On connection loss, retries with exponential backoff.
 * Returns an abort function to permanently close the stream.
 */
export function startEventStream(
  client: ReplIpcClient,
  initial: SessionState,
  onUpdate: StreamCallback,
  onError: ErrorCallback,
): () => void {
  let aborted = false
  let currentState = initial

  void (async () => {
    while (!aborted) {
      try {
        await runStream(client, initial.lastSequence, (envelope) => {
          if (aborted) return

          const newState = applyEvents(
            currentState,
            [envelope],
            Math.max(currentState.lastSequence, envelope.sequence),
          )
          currentState = newState
          onUpdate(currentState)
        })

        // Stream ended cleanly (daemon closed it). Retry.
        if (!aborted) {
          await delay(1000)
        }
      } catch (error) {
        if (aborted) return

        onError(
          error instanceof Error ? error : new Error(String(error)),
        )

        // Exponential backoff
        for (let attempt = 1; attempt <= 5; attempt++) {
          if (aborted) return
          await delay(Math.min(BASE_DELAY_MS * 2 ** attempt, MAX_DELAY_MS))
        }
      }
    }
  })()

  return () => {
    aborted = true
  }
}

/**
 * Runs a single stream session. Iterates over watchEvents until
 * the stream closes or errors.
 */
async function runStream(
  client: ReplIpcClient,
  afterSequence: number,
  onEvent: (envelope: ReplEventEnvelope) => void,
): Promise<void> {
  // Refresh the snapshot to get the latest sequence
  try {
    const snapshot = await client.getSnapshot()
    afterSequence = snapshot.last_sequence
  } catch {
    // Use the provided sequence if snapshot fails
  }

  const stream = client.watchEvents(afterSequence)

  for await (const envelope of stream) {
    onEvent(envelope)
  }
}