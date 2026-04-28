// infrastructure/ipc/ElectronReplIpcClient.ts
// Implements ReplIpcClient using the preload-exposed window.replApi bridge.

import './globals' // side-effect: registers Window.replApi type
import type { ReplIpcClient, ReplCommandResult, ReplEventsBatch } from '@/domain'
import type { ReplSessionSnapshot, ReplEventEnvelope } from '@/domain'

// ---------------------------------------------------------------------------
// Pull-based watch implementation (wraps push events from main process)
// ---------------------------------------------------------------------------

class WatchIterator implements AsyncIterable<ReplEventEnvelope> {
  private done = false
  private buffer: ReplEventEnvelope[] = []
  private resolveNext: ((value: IteratorResult<ReplEventEnvelope>) => void) | null = null
  private unsubscribe: (() => void) | null = null
  private streamId: string | null = null

  constructor(private readonly afterSequence: number) {}

  [Symbol.asyncIterator](): AsyncIterator<ReplEventEnvelope> {
    return {
      next: async (): Promise<IteratorResult<ReplEventEnvelope>> => {
        await this.ensureStarted()

        if (this.done && this.buffer.length === 0) {
          return { done: true, value: undefined }
        }

        if (this.buffer.length > 0) {
          return { done: false, value: this.buffer.shift()! }
        }

        return new Promise((resolve) => {
          this.resolveNext = resolve
        })
      },

      return: async (): Promise<IteratorResult<ReplEventEnvelope>> => {
        this.close()
        return { done: true, value: undefined }
      },
    }
  }

  private async ensureStarted(): Promise<void> {
    if (this.unsubscribe) return

    // Tell main process to start the coddy watch stream
    const streamId = (await window.replApi.invoke(
      'repl:watch-start',
      this.afterSequence,
    )) as string | { streamId: string }

    this.streamId = typeof streamId === 'string' ? streamId : streamId.streamId

    this.unsubscribe = window.replApi.on(
      'repl:watch-event',
      (data: unknown) => {
        const payload = data as {
          streamId: string
          done?: boolean
          event?: ReplEventEnvelope
          last_sequence?: number
        }

        if (payload.streamId !== this.streamId) return

        if (payload.done) {
          this.done = true
          this.resolveNext?.({ done: true, value: undefined })
          return
        }

        const event = payload.event
        if (event) {
          this.buffer.push(event)
          this.resolveNext?.({ done: false, value: this.buffer.shift()! })
        }
      },
    )
  }

  close(): void {
    if (this.streamId) {
      window.replApi.invoke('repl:watch-close', this.streamId)
    }
    this.unsubscribe?.()
    this.done = true
  }
}

// ---------------------------------------------------------------------------
// Client implementation
// ---------------------------------------------------------------------------

export class ElectronReplIpcClient implements ReplIpcClient {
  async getSnapshot(): Promise<ReplSessionSnapshot> {
    return (await window.replApi.invoke('repl:snapshot')) as ReplSessionSnapshot
  }

  async getEventsAfter(afterSequence: number): Promise<ReplEventsBatch> {
    const raw = (await window.replApi.invoke('repl:events-after', afterSequence)) as {
      events: ReplEventEnvelope[]
      last_sequence: number
    }
    return {
      events: raw.events,
      lastSequence: raw.last_sequence,
    }
  }

  watchEvents(afterSequence: number): AsyncIterable<ReplEventEnvelope> {
    return new WatchIterator(afterSequence)
  }

  async ask(text: string): Promise<ReplCommandResult> {
    return (await window.replApi.invoke('repl:ask', text)) as ReplCommandResult
  }

  async voiceTurn(transcript: string): Promise<ReplCommandResult> {
    return (await window.replApi.invoke(
      'repl:voice-turn',
      transcript,
    )) as ReplCommandResult
  }

  async stopActiveRun(): Promise<void> {
    await window.replApi.invoke('repl:stop-active-run')
  }

  async stopSpeaking(): Promise<void> {
    await window.replApi.invoke('repl:stop-speaking')
  }

  async captureVoice(): Promise<ReplCommandResult> {
    return (await window.replApi.invoke(
      'voice:capture',
    )) as ReplCommandResult
  }
}