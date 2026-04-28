// presentation/components/VoiceButton.tsx
// Voice input button. The caller owns the capture implementation because
// Electron capture dispatches VoiceTurn directly through the daemon.

import { useState, useCallback, useEffect } from 'react'
import type { ReplCommandResult } from '@/domain'
import { Icon } from './Icon'

interface Props {
  onCapture: () => Promise<ReplCommandResult>
  disabled?: boolean
}

export function VoiceButton({ onCapture, disabled = false }: Props) {
  const [capturing, setCapturing] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const handleCapture = useCallback(async () => {
    if (capturing || disabled) return

    setCapturing(true)
    setError(null)

    try {
      const result = await onCapture()
      if (result.error) {
        setError(result.error.message)
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setCapturing(false)
    }
  }, [capturing, disabled, onCapture])

  useEffect(() => {
    if (!error) return undefined

    const timer = setTimeout(() => {
      setError(null)
    }, 3000)

    return () => clearTimeout(timer)
  }, [error])

  return (
    <button
      type="button"
      onClick={handleCapture}
      disabled={disabled || capturing}
      className={`flex h-10 w-10 flex-shrink-0 items-center justify-center rounded-full border transition-colors ${
        capturing
          ? 'animate-pulse border-red-400/40 bg-red-500/15 text-red-300'
          : error
            ? 'border-yellow-400/40 bg-yellow-500/15 text-yellow-300'
            : 'border-outline-variant/70 bg-surface-container/60 text-on-surface-variant hover:border-primary/50 hover:text-primary'
      } disabled:opacity-30`}
      title={capturing ? 'Recording...' : error ?? 'Voice input'}
      aria-label={capturing ? 'Recording voice' : 'Voice input'}
    >
      <Icon name={error ? 'alert' : 'mic'} className="h-4 w-4" />
    </button>
  )
}
