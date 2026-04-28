// application/SettingsStore.ts
// Simple JSON persistence for user preferences (model, voice, theme, mode).
// Uses localStorage. Falls back to in-memory default when not in browser.

import type { ModelRef, ReplMode } from '@/domain'

export interface UserSettings {
  selectedModel: ModelRef
  mode: ReplMode
  voiceEnabled: boolean
  voiceMuted: boolean
}

const STORAGE_KEY = 'coddy:settings'

const DEFAULT_SETTINGS: UserSettings = {
  selectedModel: { provider: 'ollama', name: 'gemma4:e2b' },
  mode: 'FloatingTerminal',
  voiceEnabled: true,
  voiceMuted: false,
}

function isBrowser(): boolean {
  return typeof window !== 'undefined' && typeof window.localStorage !== 'undefined'
}

export function loadSettings(): UserSettings {
  if (!isBrowser()) return { ...DEFAULT_SETTINGS }

  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return { ...DEFAULT_SETTINGS }

    const parsed = JSON.parse(raw) as Partial<UserSettings>
    return {
      selectedModel: parsed.selectedModel ?? DEFAULT_SETTINGS.selectedModel,
      mode: parsed.mode ?? DEFAULT_SETTINGS.mode,
      voiceEnabled: parsed.voiceEnabled ?? DEFAULT_SETTINGS.voiceEnabled,
      voiceMuted: parsed.voiceMuted ?? DEFAULT_SETTINGS.voiceMuted,
    }
  } catch {
    return { ...DEFAULT_SETTINGS }
  }
}

export function saveSettings(settings: Partial<UserSettings>): void {
  if (!isBrowser()) return

  try {
    const current = loadSettings()
    const merged = { ...current, ...settings }
    localStorage.setItem(STORAGE_KEY, JSON.stringify(merged))
  } catch {
    // Storage full or unavailable — silently fail
  }
}