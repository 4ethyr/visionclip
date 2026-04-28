// __tests__/infrastructure/integration.test.ts
// Integration test: simulates the Electron main→renderer IPC bridge
// without actually spawning Electron or the coddy CLI.
//
// We test the full flow:
//   ReplIpcClient (renderer) ↔ invoke/on ↔ simulated ipcBridge handlers

import { describe, it, expect, beforeEach } from 'vitest'
import type { ReplIpcClient, ReplCommandResult } from '@/domain'
import type {
  ModelRef,
  ModelRole,
  AssessmentPolicy,
  ReplEvent,
  ReplEventEnvelope,
  ReplMode,
  ReplSessionSnapshot,
  ReplSessionSnapshotSession,
  ScreenAssistMode,
} from '@/domain'

// ---------------------------------------------------------------------------
// Simulated IPC bridge — mirrors the production ipcBridge.ts handlers
// ---------------------------------------------------------------------------

/** In-memory store for the simulated daemon */
interface SimDaemon {
  currentSequence: number
  events: ReplEventEnvelope[]
  snapshotSession: ReplSessionSnapshotSession
  commands: string[]
}

function createSimDaemon(): SimDaemon {
  return {
    currentSequence: 0,
    events: [],
    commands: [],
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

        case 'repl:stop-speaking':
          daemon.commands.push('stop-speaking')
          return Promise.resolve({ ok: true })

        case 'repl:stop-active-run':
          daemon.commands.push('stop-active-run')
          return Promise.resolve({ ok: true })

        case 'repl:select-model': {
          const model = args[0] as ModelRef
          const role = args[1] as ModelRole
          daemon.commands.push(
            `select-model:${role}:${model.provider}/${model.name}`,
          )
          pushEvent(daemon, watchListeners, {
            ModelSelected: { model, role },
          })
          if (role === 'Chat') {
            daemon.snapshotSession.selected_model = model
          }
          return Promise.resolve({
            text: `Modelo ${model.provider}/${model.name} selecionado.`,
          })
        }

        case 'repl:open-ui': {
          const mode = args[0] as ReplMode
          daemon.commands.push(`open-ui:${mode}`)
          daemon.snapshotSession.mode = mode
          pushEvent(daemon, watchListeners, {
            OverlayShown: { mode },
          })
          return Promise.resolve({
            text: `Modo ${mode} aberto.`,
          })
        }

        case 'repl:capture-and-explain': {
          const mode = args[0] as ScreenAssistMode
          const policy = args[1] as AssessmentPolicy
          const allowed = !(
            policy === 'UnknownAssessment'
            || (policy === 'RestrictedAssessment' && mode === 'MultipleChoice')
          )
          daemon.commands.push(`capture-and-explain:${mode}:${policy}`)
          pushEvent(daemon, watchListeners, {
            PolicyEvaluated: {
              policy,
              allowed,
            },
          })
          if (policy === 'UnknownAssessment') {
            daemon.snapshotSession.status = 'AwaitingConfirmation'
          }
          if (!allowed && policy === 'RestrictedAssessment') {
            return Promise.resolve({
              error: {
                code: 'assessment_policy_blocked',
                message:
                  'restricted assessments must not receive final answers or complete code',
              },
            })
          }
          return Promise.resolve({
            text: 'CaptureAndExplain solicitado.',
          })
        }

        case 'repl:dismiss-confirmation':
          daemon.commands.push('dismiss-confirmation')
          daemon.snapshotSession.status = 'Idle'
          pushEvent(daemon, watchListeners, {
            ConfirmationDismissed: {},
          })
          return Promise.resolve({
            text: 'Confirmação dispensada.',
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
      await sim.invoke('repl:stop-active-run')
    },

    async stopSpeaking() {
      await sim.invoke('repl:stop-speaking')
    },

    async selectModel(model: ModelRef, role: ModelRole) {
      return (await sim.invoke(
        'repl:select-model',
        model,
        role,
      )) as ReplCommandResult
    },

    async openUi(mode: ReplMode) {
      return (await sim.invoke('repl:open-ui', mode)) as ReplCommandResult
    },

    async captureAndExplain(
      mode: ScreenAssistMode,
      policy: AssessmentPolicy,
    ) {
      return (await sim.invoke(
        'repl:capture-and-explain',
        mode,
        policy,
      )) as ReplCommandResult
    },

    async dismissConfirmation() {
      return (await sim.invoke(
        'repl:dismiss-confirmation',
      )) as ReplCommandResult
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

  describe('stop commands', () => {
    it('routes stopSpeaking to the speech cancellation channel', async () => {
      await client.stopSpeaking()

      expect(daemon.commands).toEqual(['stop-speaking'])
    })

    it('routes stopActiveRun to the run cancellation channel', async () => {
      await client.stopActiveRun()

      expect(daemon.commands).toEqual(['stop-active-run'])
    })
  })

  describe('model and UI commands', () => {
    it('selects the chat model and emits a ModelSelected event', async () => {
      const model = { provider: 'ollama', name: 'qwen2.5:0.5b' }

      const result = await client.selectModel(model, 'Chat')
      const snapshot = await client.getSnapshot()

      expect(result.text).toContain('qwen2.5')
      expect(snapshot.session.selected_model).toEqual(model)
      expect(daemon.commands).toEqual([
        'select-model:Chat:ollama/qwen2.5:0.5b',
      ])
      expect(daemon.events.at(-1)?.event).toEqual({
        ModelSelected: { model, role: 'Chat' },
      })
    })

    it('opens desktop UI mode and stores it in the snapshot', async () => {
      const result = await client.openUi('DesktopApp')
      const snapshot = await client.getSnapshot()

      expect(result.text).toContain('DesktopApp')
      expect(snapshot.session.mode).toBe('DesktopApp')
      expect(daemon.commands).toEqual(['open-ui:DesktopApp'])
      expect(daemon.events.at(-1)?.event).toEqual({
        OverlayShown: { mode: 'DesktopApp' },
      })
    })

    it('routes screen assist through policy evaluation events', async () => {
      const result = await client.captureAndExplain(
        'ExplainVisibleScreen',
        'UnknownAssessment',
      )

      expect(result.text).toContain('CaptureAndExplain')
      expect(daemon.commands).toEqual([
        'capture-and-explain:ExplainVisibleScreen:UnknownAssessment',
      ])
      expect(daemon.events.at(-1)?.event).toEqual({
        PolicyEvaluated: {
          policy: 'UnknownAssessment',
          allowed: false,
        },
      })
    })

    it('dismisses a pending confirmation through a structured event', async () => {
      await client.captureAndExplain(
        'ExplainVisibleScreen',
        'UnknownAssessment',
      )

      const result = await client.dismissConfirmation()
      const snapshot = await client.getSnapshot()

      expect(result.text).toContain('dispensada')
      expect(snapshot.session.status).toBe('Idle')
      expect(daemon.commands).toEqual([
        'capture-and-explain:ExplainVisibleScreen:UnknownAssessment',
        'dismiss-confirmation',
      ])
      expect(daemon.events.at(-1)?.event).toEqual({
        ConfirmationDismissed: {},
      })
    })

    it('returns structured policy errors instead of throwing for blocked screen assist', async () => {
      const result = await client.captureAndExplain(
        'MultipleChoice',
        'RestrictedAssessment',
      )

      expect(result.error).toEqual({
        code: 'assessment_policy_blocked',
        message:
          'restricted assessments must not receive final answers or complete code',
      })
      expect(daemon.commands).toEqual([
        'capture-and-explain:MultipleChoice:RestrictedAssessment',
      ])
      expect(daemon.events.at(-1)?.event).toEqual({
        PolicyEvaluated: {
          policy: 'RestrictedAssessment',
          allowed: false,
        },
      })
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
