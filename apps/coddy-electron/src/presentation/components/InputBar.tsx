// presentation/components/InputBar.tsx
// Terminal-style textarea input: Enter sends, Shift+Enter newlines, auto-resize.

import { useState, useRef, useCallback, type KeyboardEvent, type ChangeEvent } from 'react'

interface Props {
  onSend: (text: string) => void
  disabled?: boolean
  placeholder?: string
}

const MAX_ROWS = 6

export function InputBar({
  onSend,
  disabled = false,
  placeholder = 'Enter command or prompt...',
}: Props) {
  const [value, setValue] = useState('')
  const textareaRef = useRef<HTMLTextAreaElement>(null)

  const submit = useCallback(() => {
    const text = value.trim()
    if (!text || disabled) return
    onSend(text)
    setValue('')
    // Reset textarea height
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto'
    }
  }, [value, disabled, onSend])

  const handleChange = useCallback(
    (e: ChangeEvent<HTMLTextAreaElement>) => {
      setValue(e.target.value)
      // Auto-resize
      const el = e.target
      el.style.height = 'auto'
      const lineHeight = 20 // approx px per line
      const maxHeight = lineHeight * MAX_ROWS
      el.style.height = `${Math.min(el.scrollHeight, maxHeight)}px`
    },
    [],
  )

  const handleKeyDown = useCallback(
    (e: KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault()
        submit()
      }
    },
    [submit],
  )

  return (
    <div className="relative flex items-start bg-surface-container rounded-2xl border border-outline-variant focus-within:border-primary transition-shadow duration-300">
      {/* Prompt arrow */}
      <span className="text-primary pl-4 pt-3 select-none font-mono text-sm leading-5">
        &gt;
      </span>

      <textarea
        ref={textareaRef}
        className="flex-1 bg-transparent border-none focus:ring-0 text-on-surface font-mono text-sm py-2.5 px-3 placeholder:text-on-surface-variant/50 resize-none overflow-y-auto leading-5"
        placeholder={placeholder}
        rows={1}
        value={value}
        onChange={handleChange}
        onKeyDown={handleKeyDown}
        disabled={disabled}
        autoFocus
      />

      {/* Mic button */}
      <button
        type="button"
        className="mt-1.5 w-8 h-8 rounded-full flex items-center justify-center text-on-surface-variant hover:text-primary transition-colors flex-shrink-0"
        title="Voice input"
        aria-label="Voice input"
      >
        <span className="text-[18px]">🎤</span>
      </button>

      {/* Send button */}
      <button
        type="button"
        onClick={submit}
        disabled={disabled || !value.trim()}
        className="mt-1.5 mr-2 w-8 h-8 rounded-full bg-primary/10 flex items-center justify-center text-primary hover:bg-primary/20 transition-colors disabled:opacity-30 disabled:cursor-not-allowed flex-shrink-0"
        title="Send"
        aria-label="Send"
      >
        <span className="text-[18px]">↑</span>
      </button>
    </div>
  )
}