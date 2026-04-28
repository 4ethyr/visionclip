// application/EventStreamer.ts
// Use case: manages a live event stream from the backend with reconnection.

import type { ReplIpcClient, ReplEventEnvelope } from '@/domain'
import type { SessionState } from './SessionManager'
import { applyEvents } from './SessionManager'

export type StreamCallback = (state: SessionState) => void
export type ErrorCallback = (error: Error) => void

const BASE_DELAY_MS = 500
const MAX_DELAY_MS = 10_000

function delay(ms: number, signal: AbortSignal): Promise<void> {
  if (signal.aborted) return Promise.resolve()

  return new Promise((resolve) => {
    const timeout = setTimeout(resolve, ms)
    signal.addEventListener(
      'abort',
      () => {
        clearTimeout(timeout)
        resolve()
      },
      { once: true },
    )
  })
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
  const abortController = new AbortController()

  void (async () => {
    while (!aborted) {
      try {
        await runStream(client, currentState.lastSequence, abortController.signal, (envelope) => {
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
          await delay(1000, abortController.signal)
        }
      } catch (error) {
        if (aborted) return

        onError(
          error instanceof Error ? error : new Error(String(error)),
        )

        // Exponential backoff
        for (let attempt = 1; attempt <= 5; attempt++) {
          if (aborted) return
          await delay(
            Math.min(BASE_DELAY_MS * 2 ** attempt, MAX_DELAY_MS),
            abortController.signal,
          )
        }
      }
    }
  })()

  return () => {
    aborted = true
    abortController.abort()
  }
}

/**
 * Runs a single stream session. Iterates over watchEvents until
 * the stream closes or errors.
 */
async function runStream(
  client: ReplIpcClient,
  afterSequence: number,
  signal: AbortSignal,
  onEvent: (envelope: ReplEventEnvelope) => void,
): Promise<void> {
  const iterator = client.watchEvents(afterSequence)[Symbol.asyncIterator]()

  try {
    while (!signal.aborted) {
      const next = await nextWithAbort(iterator, signal)
      if (next.done) return
      onEvent(next.value)
    }
  } finally {
    await iterator.return?.()
  }
}

function nextWithAbort(
  iterator: AsyncIterator<ReplEventEnvelope>,
  signal: AbortSignal,
): Promise<IteratorResult<ReplEventEnvelope>> {
  if (signal.aborted) {
    return Promise.resolve({ done: true, value: undefined })
  }

  return new Promise((resolve, reject) => {
    const onAbort = () => resolve({ done: true, value: undefined })
    signal.addEventListener('abort', onAbort, { once: true })

    iterator
      .next()
      .then(resolve, reject)
      .finally(() => {
        signal.removeEventListener('abort', onAbort)
      })
  })
}
