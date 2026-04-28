import { afterEach, describe, expect, it, vi } from 'vitest'
import type { ReplEventEnvelope, ReplIpcClient, ReplSessionSnapshot } from '@/domain'
import { createLocalSession, startEventStream } from '@/application'

function envelope(sequence: number): ReplEventEnvelope {
  return {
    sequence,
    session_id: 'session-1',
    run_id: null,
    captured_at_unix_ms: 1_775_000_000_000 + sequence,
    event: { VoiceListeningStarted: {} },
  }
}

function snapshot(lastSequence: number): ReplSessionSnapshot {
  const state = createLocalSession()
  return {
    session: state.session,
    last_sequence: lastSequence,
  }
}

describe('EventStreamer', () => {
  afterEach(() => {
    vi.useRealTimers()
  })

  it('reconnects from the latest applied sequence instead of the initial cursor', async () => {
    vi.useFakeTimers()

    const watchedAfter: number[] = []
    const client: ReplIpcClient = {
      getSnapshot: vi.fn().mockResolvedValue(snapshot(0)),
      getEventsAfter: vi.fn(),
      watchEvents(afterSequence: number): AsyncIterable<ReplEventEnvelope> {
        watchedAfter.push(afterSequence)
        const call = watchedAfter.length

        return {
          async *[Symbol.asyncIterator]() {
            if (call === 1) {
              yield envelope(1)
            }
          },
        }
      },
      ask: vi.fn(),
      voiceTurn: vi.fn(),
      stopActiveRun: vi.fn(),
      stopSpeaking: vi.fn(),
      selectModel: vi.fn(),
      openUi: vi.fn(),
      captureAndExplain: vi.fn(),
      dismissConfirmation: vi.fn(),
      captureVoice: vi.fn(),
    }

    const abort = startEventStream(
      client,
      createLocalSession(),
      () => {},
      () => {},
    )

    await vi.waitFor(() => {
      expect(watchedAfter).toEqual([0])
    })

    await vi.runOnlyPendingTimersAsync()

    await vi.waitFor(() => {
      expect(watchedAfter).toEqual([0, 1])
    })

    abort()
  })

  it('closes the active watch iterator when aborted', async () => {
    const close = vi.fn()
    let watchStarted = false
    const client: ReplIpcClient = {
      getSnapshot: vi.fn().mockResolvedValue(snapshot(0)),
      getEventsAfter: vi.fn(),
      watchEvents(): AsyncIterable<ReplEventEnvelope> {
        watchStarted = true
        return {
          [Symbol.asyncIterator]() {
            return {
              next: () => new Promise<IteratorResult<ReplEventEnvelope>>(() => {}),
              return: async () => {
                close()
                return { done: true, value: undefined }
              },
            }
          },
        }
      },
      ask: vi.fn(),
      voiceTurn: vi.fn(),
      stopActiveRun: vi.fn(),
      stopSpeaking: vi.fn(),
      selectModel: vi.fn(),
      openUi: vi.fn(),
      captureAndExplain: vi.fn(),
      dismissConfirmation: vi.fn(),
      captureVoice: vi.fn(),
    }

    const abort = startEventStream(
      client,
      createLocalSession(),
      () => {},
      () => {},
    )

    await vi.waitFor(() => {
      expect(watchStarted).toBe(true)
    })

    abort()

    await vi.waitFor(() => {
      expect(close).toHaveBeenCalledOnce()
    })
  })
})
