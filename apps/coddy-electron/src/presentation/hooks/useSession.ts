// presentation/hooks/useSession.ts
// Hook: manages the full REPL session lifecycle.
// Loads snapshot, starts event stream, exposes state + actions.

import { useState, useEffect, useCallback, useRef } from 'react'
import type {
  ModelRef,
  ModelRole,
  ReplCommandResult,
  ReplMode,
  ReplSession,
  AssessmentPolicy,
  ScreenAssistMode,
} from '@/domain'
import type { SessionState } from '@/application'
import {
  initializeSession,
  createLocalSession,
  startEventStream,
  sendAsk,
  cancelRun,
  cancelSpeech,
  selectModel,
  openUi,
  captureVoice,
  captureAndExplain,
  dismissConfirmation,
} from '@/application'
import { useReplClient } from './useReplClient'

export interface UseSessionReturn {
  session: ReplSession
  lastSequence: number
  /** True while still connecting / loading the first snapshot */
  connecting: boolean
  /** True when the daemon stream disconnected and we're retrying */
  reconnecting: boolean
  error: string | null

  /** Send a text question */
  ask: (text: string) => Promise<void>

  /** Stop the current generation */
  cancelRun: () => Promise<void>

  /** Stop TTS playback */
  cancelSpeech: () => Promise<void>

  /** Select a model for the requested role */
  selectModel: (model: ModelRef, role?: ModelRole) => Promise<void>

  /** Switch the REPL UI mode through the daemon */
  openUi: (mode: ReplMode) => Promise<void>

  /** Capture one voice turn; the backend dispatches the transcript itself */
  captureVoice: () => Promise<ReplCommandResult>

  /** Start a policy-aware screen assistance flow */
  captureAndExplain: (
    mode: ScreenAssistMode,
    policy?: AssessmentPolicy,
  ) => Promise<void>

  /** Dismiss a pending policy confirmation without sending prompt text */
  dismissConfirmation: () => Promise<void>

  /** Manually retry connection to the daemon */
  reconnect: () => void
}

export function useSession(): UseSessionReturn {
  const client = useReplClient()
  const [state, setState] = useState<SessionState>(createLocalSession())
  const [connecting, setConnecting] = useState(true)
  const [reconnecting, setReconnecting] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const abortRef = useRef<(() => void) | null>(null)
  const initCountRef = useRef(0)

  // Initialize: fetch snapshot, then start watching
  const init = useCallback(() => {
    abortRef.current?.()

    const count = ++initCountRef.current
    let cancelled = false

    setConnecting(true)
    setError(null)

    void (async () => {
      try {
        const initial = await initializeSession(client)
        if (cancelled || count !== initCountRef.current) return

        setState(initial)
        setConnecting(false)

        // Start live event stream
        abortRef.current = startEventStream(
          client,
          initial,
          (newState) => {
            if (!cancelled && count === initCountRef.current) {
              setState(newState)
              setReconnecting(false)
            }
          },
          (err) => {
            if (!cancelled && count === initCountRef.current) {
              setError(err.message)
              setReconnecting(true)
            }
          },
        )
      } catch (err) {
        if (!cancelled && count === initCountRef.current) {
          const msg = err instanceof Error ? err.message : String(err)
          setError(msg)
          setConnecting(false)
          setReconnecting(true)
        }
      }
    })()

    return () => {
      cancelled = true
    }
  }, [client])

  // Initial load
  useEffect(() => {
    const cleanup = init()
    return () => {
      cleanup?.()
      abortRef.current?.()
    }
  }, [init])

  const ask = useCallback(
    async (text: string) => {
      try {
        await sendAsk(client, text)
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err))
      }
    },
    [client],
  )

  const handleCancelRun = useCallback(async () => {
    await cancelRun(client)
  }, [client])

  const handleCancelSpeech = useCallback(async () => {
    await cancelSpeech(client)
  }, [client])

  const handleSelectModel = useCallback(
    async (model: ModelRef, role: ModelRole = 'Chat') => {
      try {
        await selectModel(client, model, role)
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err))
      }
    },
    [client],
  )

  const handleOpenUi = useCallback(
    async (mode: ReplMode) => {
      try {
        await openUi(client, mode)
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err))
      }
    },
    [client],
  )

  const handleCaptureVoice = useCallback(async (): Promise<ReplCommandResult> => {
    try {
      return await captureVoice(client)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      setError(message)
      return { error: { code: 'VOICE_CAPTURE_FAILED', message } }
    }
  }, [client])

  const handleCaptureAndExplain = useCallback(
    async (
      mode: ScreenAssistMode,
      policy: AssessmentPolicy = state.session.policy,
    ) => {
      try {
        await captureAndExplain(client, mode, policy)
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err))
      }
    },
    [client, state.session.policy],
  )

  const handleDismissConfirmation = useCallback(async () => {
    try {
      await dismissConfirmation(client)
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    }
  }, [client])

  return {
    session: state.session,
    lastSequence: state.lastSequence,
    connecting,
    reconnecting,
    error,
    ask,
    cancelRun: handleCancelRun,
    cancelSpeech: handleCancelSpeech,
    selectModel: handleSelectModel,
    openUi: handleOpenUi,
    captureVoice: handleCaptureVoice,
    captureAndExplain: handleCaptureAndExplain,
    dismissConfirmation: handleDismissConfirmation,
    reconnect: init,
  }
}
