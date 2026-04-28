// presentation/views/FloatingTerminal/FloatingTerminal.tsx
// Main REPL view: floating terminal with glass aesthetic.
// Matches the visual reference in repl_ui/floating_terminal_coding_interaction/

import { useRef, useEffect, useCallback } from 'react'
import { useSessionContext } from '@/presentation/hooks'
import { MessageBubble } from '@/presentation/components/MessageBubble'
import { StatusIndicator } from '@/presentation/components/StatusIndicator'
import { InputBar } from '@/presentation/components/InputBar'
import { ModelSelector } from '@/presentation/components/ModelSelector'
import { VoiceButton } from '@/presentation/components/VoiceButton'
import { StreamingText } from '@/presentation/components/StreamingText'
import { AssessmentConfirmModal } from '@/presentation/components/AssessmentConfirmModal'

export function FloatingTerminal() {
  const {
    session,
    connecting,
    reconnecting,
    error,
    ask,
    reconnect,
    selectModel,
    openUi,
  } =
    useSessionContext()
  const messagesEndRef = useRef<HTMLDivElement>(null)

  // Auto-scroll to bottom on new messages or streaming tokens
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [session.messages.length, session.streaming_text])

  const handleClose = useCallback(() => {
    if (typeof window !== 'undefined' && window.replApi) {
      void window.replApi.invoke('window:close')
    }
  }, [])

  const handleMinimize = useCallback(() => {
    if (typeof window !== 'undefined' && window.replApi) {
      void window.replApi.invoke('window:minimize')
    }
  }, [])

  return (
    <main className="glass-panel rounded-xl overflow-hidden flex flex-col h-screen max-h-[800px]">
      {/* ── Header (draggable for frameless window) ── */}
      <header className="bg-background/60 backdrop-blur-xl border-b border-primary/20 flex justify-between items-center w-full px-4 py-2 flex-shrink-0">
        <div className="flex items-center gap-3">
          <span className="text-primary text-lg font-mono">&gt;_</span>
          <span className="font-bold text-primary tracking-wider text-sm">
            CODDY_REPL
          </span>
        </div>
        <div className="flex items-center gap-3">
          <StatusIndicator status={session.status} />
          <ModelSelector
            model={session.selected_model}
            onSelect={(model) => {
              void selectModel(model, 'Chat')
            }}
          />
          <button
            type="button"
            onClick={() => {
              void openUi('DesktopApp')
            }}
            className="text-xs text-on-surface-variant hover:text-primary border border-outline-variant hover:border-primary/30 rounded-full px-2.5 py-0.5 transition-colors"
            title="Open desktop mode"
          >
            Desktop
          </button>
          {/* Window controls */}
          <div className="flex items-center gap-1 ml-2">
            <button
              type="button"
              onClick={handleMinimize}
              className="w-5 h-5 rounded-full flex items-center justify-center text-on-surface-variant/60 hover:text-on-surface text-xs bg-surface-container-high/50 hover:bg-surface-container-high transition-colors"
              title="Minimize"
              aria-label="Minimize"
            >
              ─
            </button>
            <button
              type="button"
              onClick={handleClose}
              className="w-5 h-5 rounded-full flex items-center justify-center text-red-400/60 hover:text-red-400 text-xs bg-surface-container-high/50 hover:bg-surface-container-high transition-colors"
              title="Close"
              aria-label="Close"
            >
              ✕
            </button>
          </div>
        </div>
      </header>

      {/* ── Messages ── */}
      <div className="flex-1 overflow-y-auto px-6 py-4 flex flex-col gap-5">
        {connecting && (
          <div className="flex items-center gap-2 text-on-surface-variant text-sm self-center py-8">
            <span className="animate-pulse">●</span>
            Connecting to daemon...
          </div>
        )}

        {error && (
          <div className="bg-red-500/10 border border-red-500/20 rounded-lg px-4 py-3 text-sm text-red-400 self-center flex items-center gap-3">
            <span>{error}</span>
            {reconnecting && (
              <button
                type="button"
                onClick={reconnect}
                className="px-2 py-0.5 rounded bg-red-500/20 hover:bg-red-500/30 text-red-300 text-xs transition-colors"
              >
                Retry
              </button>
            )}
          </div>
        )}

        {session.messages.map((msg) => (
          <MessageBubble key={msg.id} message={msg} />
        ))}

        {/* ── Streaming indicator ── */}
        {session.streaming_text && (
          <div className="flex gap-3 items-start max-w-3xl">
            <div className="w-8 h-8 rounded-full bg-primary/10 flex items-center justify-center border border-primary/30 flex-shrink-0">
              <span className="text-sm">🤖</span>
            </div>
            <div className="bg-surface-container border border-primary/10 rounded-xl rounded-tl-none px-4 py-3 max-w-[80%]">
              <p className="text-sm text-on-surface whitespace-pre-wrap break-words">
                <StreamingText text={session.streaming_text} />
                <span className="streaming-cursor" />
              </p>
            </div>
          </div>
        )}

        <div ref={messagesEndRef} />
      </div>

      {/* ── Input ── */}
      <div className="flex items-center gap-2 px-6 py-4 border-t border-primary/15 bg-background/60 backdrop-blur-md flex-shrink-0">
        <div className="flex-1">
          <InputBar
            onSend={ask}
            disabled={connecting || session.status === 'Streaming' || session.status === 'Thinking'}
            placeholder="Ask anything..."
          />
        </div>
        <VoiceButton
          onTranscript={(t) => ask(t)}
          disabled={connecting}
        />
      </div>

      {/* ── Assessment confirmation modal ── */}
      {session.status === 'AwaitingConfirmation' && (
        <AssessmentConfirmModal
          onConfirm={() => {
            // User confirmed — allow the pending request through ask
            ask('I confirm AI assistance')
          }}
          onDismiss={() => {
            // User dismissed — send a cancel
            ask('/cancel-policy')
          }}
        />
      )}
    </main>
  )
}
