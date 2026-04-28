// main/ipcBridge.ts
// Electron main process: spawns coddy CLI and bridges to renderer via IPC.

import { spawn, ChildProcess } from 'child_process'
import { createInterface } from 'readline'
import { ipcMain, BrowserWindow } from 'electron'

const CODDY_BIN = process.env.CODDY_BIN || 'coddy'

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

function coddySpawn(args: string[]): ChildProcess {
  const child = spawn(CODDY_BIN, args, {
    stdio: ['ignore', 'pipe', 'pipe'],
  })

  child.stderr?.on('data', (chunk: Buffer) => {
    console.error(`[coddy stderr] ${chunk.toString().trim()}`)
  })

  return child
}

async function readJson(child: ChildProcess): Promise<unknown> {
  return new Promise((resolve, reject) => {
    let stdout = ''
    child.stdout?.on('data', (chunk: Buffer) => { stdout += chunk.toString() })

    child.on('close', (code) => {
      if (code !== 0) {
        reject(new Error(`coddy exited ${code}`))
        return
      }
      try {
        resolve(JSON.parse(stdout.trim()))
      } catch {
        resolve(stdout.trim())
      }
    })

    child.on('error', reject)
  })
}

// ---------------------------------------------------------------------------
// Active stream tracking (for reaping on window close)
// ---------------------------------------------------------------------------

const activeStreams = new Map<string, ChildProcess>()

function reapStream(streamId: string): void {
  const child = activeStreams.get(streamId)
  if (child) {
    child.kill()
    activeStreams.delete(streamId)
  }
}

// ---------------------------------------------------------------------------
// IPC Handler registration
// ---------------------------------------------------------------------------

export function registerIpcHandlers(): void {
  // ---- Window controls ----
  ipcMain.handle('window:close', (event) => {
    BrowserWindow.fromWebContents(event.sender)?.close()
  })

  ipcMain.handle('window:minimize', (event) => {
    BrowserWindow.fromWebContents(event.sender)?.minimize()
  })

  // ---- Snapshot ----
  ipcMain.handle('repl:snapshot', async () => {
    return readJson(coddySpawn(['session', 'snapshot']))
  })

  // ---- Incremental events ----
  ipcMain.handle('repl:events-after', async (_event, afterSequence: number) => {
    return readJson(
      coddySpawn(['session', 'events', '--after', String(afterSequence)]),
    )
  })

  // ---- Watch (streaming) ----
  ipcMain.handle('repl:watch-start', async (_event, afterSequence: number) => {
    const streamId = String(Math.random()).slice(2, 10)
    const child = coddySpawn([
      'session', 'watch', '--after', String(afterSequence),
    ])

    activeStreams.set(streamId, child)
    void pumpWatchStream(streamId, child)

    return { streamId }
  })

  ipcMain.handle('repl:watch-close', async (_event, streamId: string) => {
    reapStream(streamId)
  })

  // ---- Commands ----
  ipcMain.handle('repl:ask', async (_event, text: string) => {
    const child = coddySpawn(['ask', text])
    const raw = await readJson(child)
    return normalizeCommandResult(raw)
  })

  // ---- Voice: capture + transcribe via coddy CLI ----
  ipcMain.handle('voice:capture', async () => {
    try {
      const child = coddySpawn(['voice', '--overlay'])
      const raw = await readJson(child)
      return normalizeCommandResult(raw)
    } catch (err) {
      return { error: { code: 'VOICE_CAPTURE_FAILED', message: String(err) } }
    }
  })

  ipcMain.handle('repl:voice-turn', async (_event, transcript: string) => {
    const child = coddySpawn(['voice', '--transcript', transcript])
    const raw = await readJson(child)
    return normalizeCommandResult(raw)
  })

  ipcMain.handle('repl:stop-speaking', async () => {
    const child = coddySpawn(['stop-speaking'])
    await readJson(child)
    return { ok: true }
  })

  ipcMain.handle('repl:stop-active-run', async () => {
    const child = coddySpawn(['stop-active-run'])
    await readJson(child)
    return { ok: true }
  })
}

// ---------------------------------------------------------------------------
// Cleanup on quit
// ---------------------------------------------------------------------------

export function cleanupStreams(): void {
  for (const [, child] of activeStreams) {
    child.kill()
  }
  activeStreams.clear()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async function pumpWatchStream(streamId: string, child: ChildProcess): Promise<void> {
  try {
    const stdout = child.stdout
    if (!stdout) return

    const rl = createInterface({ input: stdout, crlfDelay: Infinity })
    for await (const line of rl) {
      try {
        const parsed = JSON.parse(line)
        for (const win of BrowserWindow.getAllWindows()) {
          win.webContents.send('repl:watch-event', { streamId, ...parsed })
        }
      } catch {
        // non-JSON line - ignore daemon logs or progress text
      }
    }
  } finally {
    for (const win of BrowserWindow.getAllWindows()) {
      win.webContents.send('repl:watch-event', { streamId, done: true })
    }
    activeStreams.delete(streamId)
  }
}

function normalizeCommandResult(raw: unknown): {
  text?: string
  summary?: string
  message?: string
  error?: { code: string; message: string }
} {
  if (typeof raw === 'string') return { text: raw }
  if (raw && typeof raw === 'object') {
    const obj = raw as Record<string, unknown>
    if ('error' in obj || 'Error' in obj) {
      const err = (obj.error ?? obj.Error) as { code?: string; message?: string } | undefined
      return { error: { code: err?.code ?? 'UNKNOWN', message: err?.message ?? String(raw) } }
    }
    if ('summary' in obj) return { text: obj.text as string, summary: obj.summary as string }
    if ('text' in obj) return { text: obj.text as string }
    return { text: JSON.stringify(raw) }
  }
  return { text: String(raw) }
}
