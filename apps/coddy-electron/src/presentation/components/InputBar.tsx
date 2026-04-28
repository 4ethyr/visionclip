// presentation/components/InputBar.tsx
// Terminal-style textarea input: Enter sends, Shift+Enter newlines, auto-resize.

import { useState, useRef, useCallback, type KeyboardEvent, type ChangeEvent } from 'react'
import { Icon } from './Icon'

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
    <div className="terminal-input relative flex items-start rounded-full border border-outline-variant/80 bg-surface-container/70 px-1 shadow-[inset_0_1px_0_rgba(255,255,255,0.04)]">
      <span className="select-none pl-4 pt-3 font-mono text-sm leading-5 text-primary drop-shadow-[0_0_8px_rgba(0,219,233,0.65)]">
        &gt;
      </span>

      <textarea
        ref={textareaRef}
        className="min-h-[42px] flex-1 resize-none overflow-y-auto border-none bg-transparent px-3 py-2.5 font-mono text-sm leading-5 text-on-surface caret-primary outline-none placeholder:text-on-surface-variant/45 focus:ring-0"
        placeholder={placeholder}
        rows={1}
        value={value}
        onChange={handleChange}
        onKeyDown={handleKeyDown}
        disabled={disabled}
      />

      <button
        type="button"
        onClick={submit}
        disabled={disabled || !value.trim()}
        className="mr-2 mt-1.5 flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-full bg-primary/10 text-primary shadow-[0_0_18px_rgba(0,219,233,0.12)] transition-colors hover:bg-primary/20 disabled:cursor-not-allowed disabled:opacity-30"
        title="Send"
        aria-label="Send"
      >
        <Icon name="send" className="h-4 w-4" />
      </button>
    </div>
  )
}
