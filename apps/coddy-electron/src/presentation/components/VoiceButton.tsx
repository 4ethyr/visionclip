// presentation/components/VoiceButton.tsx
// Voice input: browser mic OR system capture via coddy CLI.
// In Electron, delegates to coddy voice --overlay for full lock+STT pipeline.

import { useState, useCallback } from 'react'
import { useReplClient } from '@/presentation/hooks'

interface Props {
  onTranscript: (text: string) => void
  disabled?: boolean
}

export function VoiceButton({ onTranscript, disabled = false }: Props) {
  const [capturing, setCapturing] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const client = useReplClient()

  const handleCapture = useCallback(async () => {
    if (capturing || disabled) return

    setCapturing(true)
    setError(null)

    try {
      const result = await client.captureVoice()
      if (result.error) {
        setError(result.error.message)
      } else if (result.text) {
        onTranscript(result.text)
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err))
    } finally {
      setCapturing(false)
    }
  }, [capturing, disabled, client, onTranscript])

  // Auto-clear error after 3s
  const [clearTimer, setClearTimer] = useState<ReturnType<typeof setTimeout> | null>(null)
  if (error && !clearTimer) {
    setClearTimer(setTimeout(() => {
      setError(null)
      setClearTimer(null)
    }, 3000))
  }
  if (!error && clearTimer) {
    clearTimeout(clearTimer)
    setClearTimer(null)
  }

  return (
    <button
      type="button"
      onClick={handleCapture}
      disabled={disabled || capturing}
      className={`w-8 h-8 rounded-full flex items-center justify-center transition-colors flex-shrink-0 ${
        capturing
          ? 'bg-red-500/20 text-red-400 animate-pulse'
          : error
            ? 'bg-yellow-500/20 text-yellow-400'
            : 'text-on-surface-variant hover:text-primary'
      } disabled:opacity-30`}
      title={capturing ? 'Recording...' : error ?? 'Voice input'}
      aria-label={capturing ? 'Recording voice' : 'Voice input'}
    >
      <span className="text-[16px]">
        {capturing ? '⏺' : error ? '⚠' : '🎤'}
      </span>
    </button>
  )
}