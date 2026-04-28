import { afterEach, describe, expect, it, vi } from 'vitest'
import type { ReplEventEnvelope } from '@/domain'
import { ElectronReplIpcClient } from '@/infrastructure/ipc'

type WatchCallback = (data: unknown) => void

function envelope(sequence: number): ReplEventEnvelope {
  return {
    sequence,
    session_id: 'session-1',
    run_id: null,
    captured_at_unix_ms: 1_775_000_000_000 + sequence,
    event: { VoiceListeningStarted: {} },
  }
}

describe('ElectronReplIpcClient', () => {
  let callback: WatchCallback | null = null

  afterEach(() => {
    callback = null
    // @ts-expect-error test cleanup removes the preload bridge mock
    delete window.replApi
  })

  it('buffers multiple watch events that arrive before the consumer asks again', async () => {
    window.replApi = {
      invoke: vi.fn().mockResolvedValue({ streamId: 'stream-1' }),
      on: vi.fn((_channel: string, cb: WatchCallback) => {
        callback = cb
        return vi.fn()
      }),
    }

    const client = new ElectronReplIpcClient()
    const iterator = client.watchEvents(0)[Symbol.asyncIterator]()
    const firstPromise = iterator.next()

    await vi.waitFor(() => {
      expect(callback).toBeTruthy()
    })

    callback?.({ streamId: 'stream-1', event: envelope(1) })
    callback?.({ streamId: 'stream-1', event: envelope(2) })

    await expect(firstPromise).resolves.toMatchObject({
      done: false,
      value: { sequence: 1 },
    })

    await expect(iterator.next()).resolves.toMatchObject({
      done: false,
      value: { sequence: 2 },
    })

    await iterator.return?.()
  })

  it('buffers replay events emitted before watch-start resolves', async () => {
    window.replApi = {
      invoke: vi.fn(async () => {
        callback?.({ streamId: 'stream-1', event: envelope(1) })
        return { streamId: 'stream-1' }
      }),
      on: vi.fn((_channel: string, cb: WatchCallback) => {
        callback = cb
        return vi.fn()
      }),
    }

    const client = new ElectronReplIpcClient()
    const iterator = client.watchEvents(0)[Symbol.asyncIterator]()

    await expect(iterator.next()).resolves.toMatchObject({
      done: false,
      value: { sequence: 1 },
    })

    await iterator.return?.()
  })

  it('unsubscribes the watch listener when watch-start fails', async () => {
    const unsubscribe = vi.fn()
    window.replApi = {
      invoke: vi.fn().mockRejectedValue(new Error('watch failed')),
      on: vi.fn(() => unsubscribe),
    }

    const client = new ElectronReplIpcClient()
    const iterator = client.watchEvents(0)[Symbol.asyncIterator]()

    await expect(iterator.next()).rejects.toThrow('watch failed')
    expect(unsubscribe).toHaveBeenCalledOnce()
  })

  it('routes model and UI commands through the preload bridge', async () => {
    const invoke = vi.fn().mockResolvedValue('ok')
    window.replApi = {
      invoke,
      on: vi.fn(),
    }

    const client = new ElectronReplIpcClient()
    await client.selectModel(
      { provider: 'ollama', name: 'qwen2.5:0.5b' },
      'Chat',
    )
    await client.openUi('DesktopApp')
    await client.captureAndExplain('MultipleChoice', 'RestrictedAssessment')
    await client.dismissConfirmation()

    expect(invoke).toHaveBeenCalledWith(
      'repl:select-model',
      { provider: 'ollama', name: 'qwen2.5:0.5b' },
      'Chat',
    )
    expect(invoke).toHaveBeenCalledWith('repl:open-ui', 'DesktopApp')
    expect(invoke).toHaveBeenCalledWith(
      'repl:capture-and-explain',
      'MultipleChoice',
      'RestrictedAssessment',
    )
    expect(invoke).toHaveBeenCalledWith('repl:dismiss-confirmation')
  })
})
