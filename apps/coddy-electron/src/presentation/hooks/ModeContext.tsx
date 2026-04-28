// presentation/hooks/ModeContext.tsx
// Lightweight context for the current UI mode (FloatingTerminal vs DesktopApp).

import {
  createContext,
  useCallback,
  useContext,
  useState,
  type ReactNode,
} from 'react'
import type { ReplMode } from '@/domain'
import { loadSettings, saveSettings } from '@/application'

interface ModeContextValue {
  mode: ReplMode
  setMode: (mode: ReplMode) => void
  /** Toggle between FloatingTerminal and DesktopApp */
  toggleMode: () => void
}

const ModeContext = createContext<ModeContextValue | null>(null)

export function ModeProvider({ children }: { children: ReactNode }) {
  const [mode, setModeState] = useState<ReplMode>(() => {
    const settings = loadSettings()
    return settings.mode
  })

  const setMode = useCallback((newMode: ReplMode) => {
    setModeState(newMode)
    saveSettings({ mode: newMode })
  }, [])

  const toggleMode = useCallback(() => {
    setMode(mode === 'FloatingTerminal' ? 'DesktopApp' : 'FloatingTerminal')
  }, [mode, setMode])

  return (
    <ModeContext.Provider value={{ mode, setMode, toggleMode }}>
      {children}
    </ModeContext.Provider>
  )
}

export function useMode(): ModeContextValue {
  const ctx = useContext(ModeContext)
  if (!ctx) {
    throw new Error('useMode must be used within a ModeProvider')
  }
  return ctx
}
