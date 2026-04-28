// presentation/hooks/useReplClient.ts
// Provides a singleton ReplIpcClient instance to all components.
// Injects the real ElectronReplIpcClient when running in Electron,
// or a fake stub for tests/storybook.

import type { ReplIpcClient } from '@/domain'
import { ElectronReplIpcClient } from '@/infrastructure/ipc'

let cachedClient: ReplIpcClient | null = null

function createClient(): ReplIpcClient {
  if (typeof window !== 'undefined' && window.replApi) {
    return new ElectronReplIpcClient()
  }
  // Fallback for testing — a stub that never resolves
  return {
    getSnapshot: () => new Promise(() => {}),
    getEventsAfter: () => new Promise(() => {}),
    watchEvents: () => ({ [Symbol.asyncIterator]: () => ({ next: () => new Promise(() => {}) }) }),
    ask: () => new Promise(() => {}),
    voiceTurn: () => new Promise(() => {}),
    stopActiveRun: () => Promise.resolve(),
    stopSpeaking: () => Promise.resolve(),
    selectModel: () => Promise.resolve({}),
    openUi: () => Promise.resolve({}),
    captureAndExplain: () => Promise.resolve({}),
    dismissConfirmation: () => Promise.resolve({}),
    captureVoice: () => Promise.resolve({}),
  }
}

export function useReplClient(): ReplIpcClient {
  // Cache in module scope for singleton access
  if (!cachedClient) {
    cachedClient = createClient()
  }
  return cachedClient
}
