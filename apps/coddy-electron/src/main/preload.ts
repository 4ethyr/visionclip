// main/preload.ts
// Thin bridge: exposes only ipcRenderer.invoke/on via contextBridge.
// Types live in infrastructure/ipc/apiTypes.ts — shared between preload and renderer.

import { contextBridge, ipcRenderer } from 'electron'

const replApi = {
  invoke(channel: string, ...args: unknown[]): Promise<unknown> {
    const validChannels = [
      'repl:snapshot',
      'repl:events-after',
      'repl:ask',
      'repl:voice-turn',
      'repl:stop-speaking',
      'repl:stop-active-run',
      'repl:select-model',
      'repl:open-ui',
      'repl:capture-and-explain',
      'repl:dismiss-confirmation',
      'repl:watch-start',
      'repl:watch-close',
      'window:close',
      'window:minimize',
      'window:maximize',
      'voice:capture',
    ]
    if (validChannels.includes(channel)) {
      return ipcRenderer.invoke(channel, ...args)
    }
    return Promise.reject(new Error(`Invalid channel: ${channel}`))
  },

  on(channel: string, callback: (...args: unknown[]) => void): () => void {
    const validChannels = ['repl:watch-event']
    if (validChannels.includes(channel)) {
      const listener = (_event: Electron.IpcRendererEvent, ...args: unknown[]) =>
        callback(...args)
      ipcRenderer.on(channel, listener)
      return () => ipcRenderer.removeListener(channel, listener)
    }
    return () => {}
  },
}

contextBridge.exposeInMainWorld('replApi', replApi)
