// presentation/hooks/SessionContext.tsx
// React context that provides the session state + actions to all components.

import { createContext, useContext, type ReactNode } from 'react'
import { useSession, type UseSessionReturn } from './useSession'

const SessionContext = createContext<UseSessionReturn | null>(null)

interface Props {
  children: ReactNode
}

export function SessionProvider({ children }: Props) {
  const session = useSession()

  return (
    <SessionContext.Provider value={session}>
      {children}
    </SessionContext.Provider>
  )
}

/**
 * Hook to access the session context. Must be used within SessionProvider.
 * Throws if called outside a provider.
 */
export function useSessionContext(): UseSessionReturn {
  const ctx = useContext(SessionContext)
  if (!ctx) {
    throw new Error(
      'useSessionContext must be used within a SessionProvider',
    )
  }
  return ctx
}