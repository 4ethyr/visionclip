// presentation/App.tsx
// Root component: wraps in SessionProvider + ModeProvider.
// AppInner switches between FloatingTerminal and DesktopApp based on mode.

import { useEffect } from 'react'
import { FloatingTerminal } from './views/FloatingTerminal'
import { DesktopApp } from './views/DesktopApp'
import { SessionProvider, useSessionContext, ModeProvider, useMode } from './hooks'

/** Inner component that has access to session + mode contexts */
function AppInner() {
  const { session, connecting } = useSessionContext()
  const { mode, setMode } = useMode()

  // Keyboard: Escape closes floating terminal
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && mode === 'FloatingTerminal') {
        if (typeof window !== 'undefined' && window.replApi) {
          void window.replApi.invoke('window:close')
        }
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [mode])

  useEffect(() => {
    if (!connecting && session.mode !== mode) {
      setMode(session.mode)
    }
  }, [connecting, mode, session.mode, setMode])

  if (connecting) return <FloatingTerminal />

  const activeMode = session.mode

  if (activeMode === 'DesktopApp') return <DesktopApp />
  return <FloatingTerminal />
}

/** Root provider wrapper */
export function App() {
  return (
    <SessionProvider>
      <ModeProvider>
        <AppInner />
      </ModeProvider>
    </SessionProvider>
  )
}
