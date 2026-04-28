// application/SettingsStore.ts
// Simple JSON persistence for user preferences (model, voice, theme, mode).
// Uses localStorage. Falls back to in-memory default when not in browser.

import type { ModelRef, ReplMode } from '@/domain'

export interface FloatingAppearanceSettings {
  blurPx: number
  transparency: number
  glassIntensity: number
  textColor: string
  accentColor: string
}

export interface UserSettings {
  selectedModel: ModelRef
  mode: ReplMode
  voiceEnabled: boolean
  voiceMuted: boolean
  floatingAppearance: FloatingAppearanceSettings
}

const STORAGE_KEY = 'coddy:settings'

export const DEFAULT_FLOATING_APPEARANCE: FloatingAppearanceSettings = {
  blurPx: 24,
  transparency: 0.58,
  glassIntensity: 0.14,
  textColor: '#e5e2e1',
  accentColor: '#00dbe9',
}

const DEFAULT_SETTINGS: UserSettings = {
  selectedModel: { provider: 'ollama', name: 'gemma4:e2b' },
  mode: 'FloatingTerminal',
  voiceEnabled: true,
  voiceMuted: false,
  floatingAppearance: { ...DEFAULT_FLOATING_APPEARANCE },
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
      floatingAppearance: normalizeFloatingAppearance(parsed.floatingAppearance),
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

export function normalizeFloatingAppearance(
  value: Partial<FloatingAppearanceSettings> | undefined,
): FloatingAppearanceSettings {
  return {
    blurPx: clampNumber(value?.blurPx, 0, 48, DEFAULT_FLOATING_APPEARANCE.blurPx),
    transparency: clampNumber(
      value?.transparency,
      0.32,
      0.92,
      DEFAULT_FLOATING_APPEARANCE.transparency,
    ),
    glassIntensity: clampNumber(
      value?.glassIntensity,
      0,
      0.32,
      DEFAULT_FLOATING_APPEARANCE.glassIntensity,
    ),
    textColor: validHexColor(value?.textColor, DEFAULT_FLOATING_APPEARANCE.textColor),
    accentColor: validHexColor(value?.accentColor, DEFAULT_FLOATING_APPEARANCE.accentColor),
  }
}

function clampNumber(
  value: number | undefined,
  min: number,
  max: number,
  fallback: number,
): number {
  if (typeof value !== 'number' || Number.isNaN(value)) return fallback
  return Math.min(max, Math.max(min, value))
}

function validHexColor(value: string | undefined, fallback: string): string {
  if (!value) return fallback
  return /^#[0-9a-f]{6}$/i.test(value) ? value : fallback
}
