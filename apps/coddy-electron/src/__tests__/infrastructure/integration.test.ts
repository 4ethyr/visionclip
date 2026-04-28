// __tests__/infrastructure/integration.test.ts
// Integration test: simulates the Electron main→renderer IPC bridge
// without actually spawning Electron or the coddy CLI.
//
// We test the full flow:
//   ReplIpcClient (renderer) ↔ invoke/on ↔ simulated ipcBridge handlers

import { describe, it, expect, beforeEach } from 'vitest'
import type { ReplIpcClient, ReplCommandResult } from '@/domain'
import type { ReplSessionSnapshot, ReplEventEnvelope, ReplEvent, ReplSessionSnapshotSession } from '@/domain'

// ---------------------------------------------------------------------------
// Simulated IPC bridge — mirrors the production ipcBridge.ts handlers
// ---------------------------------------------------------------------------

/** In-memory store for the simulated daemon */
interface SimDaemon {
  currentSequence: number
  events: ReplEventEnvelope[]
  snapshotSession: ReplSessionSnapshotSession
}

function createSimDaemon(): SimDaemon {
  return {
    currentSequence: 0,
    events: [],
    snapshotSession: {
      id: 'sim-session-uuid',
      mode: 'FloatingTerminal',
      status: 'Idle',
      policy: 'UnknownAssessment',
      selected_model: { provider: 'ollama', name: 'test-model' },
      voice: { enabled: true, speaking: false, muted: false },
      screen_context: null,
      workspace_context: [],
      messages: [],
      active_run: null,
    } satisfies ReplSessionSnapshotSession,
  }
}

/** Simulates the main process ipcBridge.ts handlers */
function createSimBridge(daemon: SimDaemon) {
  const watchListeners: Array<(data: unknown) => void> = []

  return {
    invoke(channel: string, ...args: unknown[]): Promise<unknown> {
      switch (channel) {
        // ---- Snapshot ----
        case 'repl:snapshot': {
          return Promise.resolve({
            session: daemon.snapshotSession,
            last_sequence: daemon.currentSequence,
          } satisfies ReplSessionSnapshot)
        }

        // ---- Incremental events ----
        case 'repl:events-after': {
          const after = args[0] as number
          const events = daemon.events.filter(
            (e) => e.sequence > after,
          )
          return Promise.resolve({
            events,
            last_sequence: daemon.currentSequence,
          })
        }

        // ---- Watch start ----
        case 'repl:watch-start': {
          // Push existing events after sequence
          const after = args[0] as number
          const replay = daemon.events.filter(
            (e) => e.sequence > after,
          )
          for (const event of replay) {
            for (const listener of watchListeners) {
              listener({ event, done: false, streamId: 'sim-stream' })
            }
          }

          return Promise.resolve({ streamId: 'sim-stream' })
        }

        case 'repl:watch-close':
          return Promise.resolve(undefined)

        // ---- Commands ----
        case 'repl:ask': {
          const text = args[0] as string
          // Simulate: daemon processes, emits events
          const runId = `run-${Date.now()}`
          pushEvent(daemon, watchListeners, {
            RunStarted: { run_id: runId },
          })
          pushEvent(daemon, watchListeners, {
            MessageAppended: {
              message: {
                id: `msg-${Date.now()}`,
                role: 'user',
                text,
              },
            },
          })
          pushEvent(daemon, watchListeners, {
            TokenDelta: { run_id: runId, text: `Echo: ${text}` },
          })
          pushEvent(daemon, watchListeners, {
            RunCompleted: { run_id: runId },
          })
          return Promise.resolve({ text: `Echo: ${text}` })
        }

        case 'voice:capture':
          // Simulated voice capture
          pushEvent(daemon, watchListeners, {
            VoiceListeningStarted: {},
          })
          pushEvent(daemon, watchListeners, {
            VoiceTranscriptFinal: { text: 'comando de voz simulado' },
          })
          pushEvent(daemon, watchListeners, {
            IntentDetected: {
              intent: 'SearchDocs',
              confidence: 0.85,
            },
          })
          return Promise.resolve({
            text: 'comando de voz simulado',
          })

        default:
          return Promise.reject(new Error(`Unknown channel: ${channel}`))
      }
    },

    on(
      channel: string,
      callback: (...args: unknown[]) => void,
    ): () => void {
      if (channel === 'repl:watch-event') {
        watchListeners.push(callback)
        return () => {
          const idx = watchListeners.indexOf(callback)
          if (idx >= 0) watchListeners.splice(idx, 1)
        }
      }
      return () => {}
    },

    /** Simulate the daemon pushing a live event */
    pushLiveEvent(event: ReplEvent): void {
      pushEvent(daemon, watchListeners, event)
    },
  }
}

function pushEvent(
  daemon: SimDaemon,
  listeners: Array<(data: unknown) => void>,
  event: ReplEvent,
): void {
  daemon.currentSequence++
  const envelope: ReplEventEnvelope = {
    sequence: daemon.currentSequence,
    session_id: 'sim-session-uuid',
    run_id: null,
    captured_at_unix_ms: Date.now(),
    event,
  }
  daemon.events.push(envelope)
  for (const listener of listeners) {
    listener({ event: envelope, done: false, streamId: 'sim-stream' })
  }
}

// ---------------------------------------------------------------------------
// SimElectronReplIpcClient — uses the sim bridge instead of window.replApi
// ---------------------------------------------------------------------------

function createSimClient(sim: ReturnType<typeof createSimBridge>): ReplIpcClient {
  return {
    async getSnapshot() {
      return (await sim.invoke('repl:snapshot')) as ReplSessionSnapshot
    },

    async getEventsAfter(afterSequence: number) {
      const raw = (await sim.invoke('repl:events-after', afterSequence)) as {
        events: ReplEventEnvelope[]
        last_sequence: number
      }
      return { events: raw.events, lastSequence: raw.last_sequence }
    },

    watchEvents(afterSequence: number): AsyncIterable<ReplEventEnvelope> {
      const stream: AsyncIterable<ReplEventEnvelope> = {
        [Symbol.asyncIterator]() {
          let done = false
          const pending: ReplEventEnvelope[] = []
          let resolveNext:
            | ((value: IteratorResult<ReplEventEnvelope>) => void)
            | null = null

          const unsubscribe = sim.on('repl:watch-event', (data: unknown) => {
            const payload = data as {
              event?: ReplEventEnvelope
              done?: boolean
            }
            if (payload.done) {
              done = true
              resolveNext?.({ done: true, value: undefined })
              return
            }
            if (payload.event) {
              if (resolveNext) {
                resolveNext({ done: false, value: payload.event })
                resolveNext = null
              } else {
                pending.push(payload.event)
              }
            }
          })

          // Replay events after the given sequence from the store
          void sim.invoke('repl:watch-start', afterSequence)

          return {
            async next(): Promise<IteratorResult<ReplEventEnvelope>> {
              if (done && pending.length === 0) {
                return { done: true, value: undefined }
              }
              if (pending.length > 0) {
                return { done: false, value: pending.shift()! }
              }
              return new Promise((resolve) => {
                resolveNext = resolve
              })
            },
            async return(): Promise<IteratorResult<ReplEventEnvelope>> {
              done = true
              unsubscribe()
              sim.invoke('repl:watch-close')
              return { done: true, value: undefined }
            },
          }
        },
      }
      return stream
    },

    async ask(text: string) {
      return (await sim.invoke('repl:ask', text)) as ReplCommandResult
    },

    async voiceTurn(transcript: string) {
      return { text: transcript }
    },

    async stopActiveRun() {
      await sim.invoke('repl:stop-speaking')
    },

    async stopSpeaking() {
      await sim.invoke('repl:stop-speaking')
    },

    async captureVoice() {
      return (await sim.invoke('voice:capture')) as ReplCommandResult
    },
  }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('IPC integration', () => {
  let daemon: SimDaemon
  let sim: ReturnType<typeof createSimBridge>
  let client: ReplIpcClient

  beforeEach(() => {
    daemon = createSimDaemon()
    sim = createSimBridge(daemon)
    client = createSimClient(sim)
  })

  describe('snapshot', () => {
    it('returns session with last_sequence', async () => {
      const snapshot = await client.getSnapshot()
      expect(snapshot.session).toBeDefined()
      expect(typeof snapshot.last_sequence).toBe('number')
      expect(snapshot.session.id).toBe('sim-session-uuid')
    })
  })

  describe('events after', () => {
    it('returns only events after the given sequence', async () => {
      // Push some events
      sim.pushLiveEvent({ VoiceListeningStarted: {} })
      sim.pushLiveEvent({
        VoiceTranscriptFinal: { text: 'hello' },
      })

      const batch = await client.getEventsAfter(1)
      expect(batch.events).toHaveLength(1)
      expect(batch.lastSequence).toBe(2)
    })

    it('returns empty for no new events', async () => {
      sim.pushLiveEvent({ VoiceListeningStarted: {} })
      const batch = await client.getEventsAfter(1)
      expect(batch.events).toHaveLength(0)
    })
  })

  describe('ask command', () => {
    it('sends text and returns result', async () => {
      const result = await client.ask('quem foi rousseau?')
      expect(result.text).toContain('rousseau')
    })
  })

  describe('voice capture', () => {
    it('captures and returns transcript', async () => {
      const result = await client.captureVoice()
      expect(result.text).toBe('comando de voz simulado')
    })
  })

  describe('event stream', () => {
    it('receives live events after subscription', async () => {
      const received: ReplEventEnvelope[] = []

      const stream = client.watchEvents(0)
      const iterator = stream[Symbol.asyncIterator]()

      // Give the stream a tick to set up listeners
      await new Promise((r) => setTimeout(r, 10))

      // Push live events
      sim.pushLiveEvent({
        VoiceTranscriptFinal: { text: 'terminal' },
      })

      const first = await iterator.next()
      expect(first.done).toBe(false)
      if (!first.done && first.value) {
        received.push(first.value)
      }

      expect(received).toHaveLength(1)
    })
  })
})