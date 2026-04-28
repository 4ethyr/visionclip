// infrastructure/ipc/globals.ts
// Augments the Window interface so the renderer sees window.replApi.
// Must be imported at least once (e.g. in the ElectronReplIpcClient module).

interface ReplApi {
  invoke(channel: string, ...args: unknown[]): Promise<unknown>
  on(channel: string, callback: (...args: unknown[]) => void): () => void
}

declare global {
  interface Window {
    replApi: ReplApi
  }
}

export {}