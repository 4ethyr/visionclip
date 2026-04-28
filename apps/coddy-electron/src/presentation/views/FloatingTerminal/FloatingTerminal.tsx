// presentation/views/FloatingTerminal/FloatingTerminal.tsx
// Main REPL view: floating terminal with glass aesthetic.
// Matches the visual reference in repl_ui/floating_terminal_coding_interaction/

import { useRef, useEffect, useCallback, useState } from 'react'
import type { CSSProperties } from 'react'
import type { ScreenAssistMode } from '@/domain'
import type { FloatingAppearanceSettings } from '@/application'
import { loadSettings, saveSettings } from '@/application'
import { useSessionContext } from '@/presentation/hooks'
import { MessageBubble } from '@/presentation/components/MessageBubble'
import { StatusIndicator } from '@/presentation/components/StatusIndicator'
import { InputBar } from '@/presentation/components/InputBar'
import { ModelSelector } from '@/presentation/components/ModelSelector'
import { VoiceButton } from '@/presentation/components/VoiceButton'
import { StreamingText } from '@/presentation/components/StreamingText'
import { AssessmentConfirmModal } from '@/presentation/components/AssessmentConfirmModal'
import { FloatingSettingsModal } from '@/presentation/components/FloatingSettingsModal'
import { Icon } from '@/presentation/components/Icon'

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
    captureVoice,
    captureAndExplain,
    dismissConfirmation,
  } =
    useSessionContext()
  const messagesEndRef = useRef<HTMLDivElement>(null)
  const [pendingScreenAssistMode, setPendingScreenAssistMode] =
    useState<ScreenAssistMode | null>(null)
  const [confirmationDismissed, setConfirmationDismissed] = useState(false)
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [appearance, setAppearance] = useState<FloatingAppearanceSettings>(
    () => loadSettings().floatingAppearance,
  )

  // Auto-scroll to bottom on new messages or streaming tokens
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [session.messages.length, session.streaming_text])

  useEffect(() => {
    if (session.status !== 'AwaitingConfirmation') {
      setConfirmationDismissed(false)
    }
  }, [session.status])

  const handleClose = useCallback(() => {
    if (typeof window !== 'undefined' && window.replApi) {
      void window.replApi.invoke('window:close')
    }
  }, [])

  const handleMaximize = useCallback(() => {
    if (typeof window !== 'undefined' && window.replApi) {
      void window.replApi.invoke('window:maximize')
    }
  }, [])

  const handleMinimize = useCallback(() => {
    if (typeof window !== 'undefined' && window.replApi) {
      void window.replApi.invoke('window:minimize')
    }
  }, [])

  const handleAppearanceChange = useCallback(
    (next: FloatingAppearanceSettings) => {
      setAppearance(next)
      saveSettings({ floatingAppearance: next })
    },
    [],
  )

  const terminalStyle = {
    '--coddy-terminal-opacity': String(appearance.transparency),
    '--coddy-terminal-blur': `${appearance.blurPx}px`,
    '--coddy-terminal-glass': String(appearance.glassIntensity),
    '--coddy-terminal-text': appearance.textColor,
    '--coddy-terminal-accent': appearance.accentColor,
  } as CSSProperties

  return (
    <main
      className="floating-terminal-shell aurora-gradient flex h-[min(800px,calc(100vh-48px))] w-[min(1120px,calc(100vw-48px))] flex-col overflow-hidden rounded-xl border border-primary/20"
      style={terminalStyle}
    >
      <header className="flex w-full flex-shrink-0 items-center justify-between border-b border-primary/20 bg-slate-950/60 px-6 py-3 shadow-[0_4px_30px_rgba(0,0,0,0.15)] backdrop-blur-xl">
        <div className="flex items-center gap-3">
          <Icon
            name="terminal"
            className="h-5 w-5 text-primary drop-shadow-[0_0_8px_rgba(0,240,255,0.55)]"
          />
          <span className="font-display text-xl font-semibold uppercase tracking-[0.18em] text-primary drop-shadow-[0_0_10px_rgba(0,240,255,0.55)]">
            CODDY_TERMINAL
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
            className="hidden items-center gap-2 rounded-full border border-outline-variant/80 bg-surface-container-high/70 px-3 py-1 font-mono text-xs text-on-surface-variant transition-colors hover:border-primary/50 hover:text-primary sm:flex"
            title="Open desktop mode"
          >
            <Icon name="desktop" className="h-3.5 w-3.5" />
            Desktop
          </button>
          <button
            type="button"
            onClick={() => {
              const mode: ScreenAssistMode = 'ExplainVisibleScreen'
              setPendingScreenAssistMode(mode)
              setConfirmationDismissed(false)
              void captureAndExplain(mode)
            }}
            className="hidden items-center gap-2 rounded-full border border-outline-variant/80 bg-surface-container-high/70 px-3 py-1 font-mono text-xs text-on-surface-variant transition-colors hover:border-primary/50 hover:text-primary md:flex"
            title="Explain visible screen"
          >
            <Icon name="screen" className="h-3.5 w-3.5" />
            Screen
          </button>
          <button
            type="button"
            className="text-on-surface-variant transition-colors hover:text-primary"
            aria-label="Sensors"
            title="Sensors"
          >
            <Icon name="sensors" className="h-5 w-5" />
          </button>
          <button
            type="button"
            onClick={() => setSettingsOpen(true)}
            className="text-on-surface-variant transition-colors hover:text-primary"
            aria-label="Settings"
            title="Settings"
          >
            <Icon name="settings" className="h-5 w-5" />
          </button>
          <div className="ml-1 flex items-center gap-1">
            <button
              type="button"
              onClick={handleMinimize}
              className="flex h-5 w-5 items-center justify-center rounded-full bg-surface-container-high/70 text-on-surface-variant/70 transition-colors hover:text-on-surface"
              title="Minimize"
              aria-label="Minimize"
            >
              <Icon name="minimize" className="h-3.5 w-3.5" />
            </button>
            <button
              type="button"
              onClick={handleMaximize}
              className="flex h-5 w-5 items-center justify-center rounded-full bg-surface-container-high/70 text-on-surface-variant/70 transition-colors hover:text-on-surface"
              title="Maximize"
              aria-label="Maximize"
            >
              <Icon name="maximize" className="h-3.5 w-3.5" />
            </button>
            <button
              type="button"
              onClick={handleClose}
              className="flex h-5 w-5 items-center justify-center rounded-full bg-red-500/10 text-red-300/70 transition-colors hover:text-red-300"
              title="Close"
              aria-label="Close"
            >
              <Icon name="close" className="h-3.5 w-3.5" />
            </button>
          </div>
        </div>
      </header>

      <div className="terminal-canvas flex-1 overflow-y-auto px-8 py-7">
        <div className="flex flex-col gap-7">
          <SystemLine text="system.initialize(context='coddy_floating');" />
          <SystemLine text={`daemon.status=${connecting ? 'connecting' : 'ready'}; model='${session.selected_model.name}';`} />
          <SystemLine text="awaiting user command." />

          {error && (
            <div className="flex items-center gap-3 rounded-lg border border-red-400/25 bg-red-500/10 px-4 py-3 font-mono text-sm text-red-300">
              <Icon name="alert" className="h-4 w-4" />
              <span className="min-w-0 flex-1 break-words">{error}</span>
              {reconnecting && (
                <button
                  type="button"
                  onClick={reconnect}
                  className="rounded border border-red-300/30 px-2 py-1 text-xs transition-colors hover:bg-red-300/10"
                >
                  retry
                </button>
              )}
            </div>
          )}

          {session.messages.map((msg) => (
            <MessageBubble key={msg.id} message={msg} />
          ))}

          {session.streaming_text && (
            <div className="flex w-full items-start gap-4">
              <div className="mt-1 flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-md border border-primary bg-primary/10 text-primary shadow-[0_0_22px_rgba(0,219,233,0.2)]">
                <Icon name="bot" className="h-4 w-4" />
              </div>
              <div className="min-w-0 flex-1 rounded-lg border border-primary/20 bg-surface-container/45 px-5 py-4">
                <div className="mb-2 font-mono text-[10px] uppercase tracking-[0.22em] text-primary/80">
                  streaming_response
                </div>
                <p className="whitespace-pre-wrap break-words font-mono text-sm leading-6 text-on-surface">
                  <StreamingText text={session.streaming_text} />
                  <span className="streaming-cursor" />
                </p>
              </div>
            </div>
          )}

          <div ref={messagesEndRef} />
        </div>
      </div>

      <div className="flex flex-shrink-0 items-center gap-3 border-t border-primary/15 bg-surface-dim/70 px-8 py-4 backdrop-blur-md">
        <div className="flex-1">
          <InputBar
            onSend={ask}
            disabled={connecting || session.status === 'Streaming' || session.status === 'Thinking'}
            placeholder="Enter command or prompt..."
          />
        </div>
        <VoiceButton onCapture={captureVoice} disabled={connecting} />
      </div>

      {session.status === 'AwaitingConfirmation' && !confirmationDismissed && (
        <AssessmentConfirmModal
          onConfirm={() => {
            const mode = pendingScreenAssistMode ?? 'ExplainVisibleScreen'
            setPendingScreenAssistMode(null)
            void captureAndExplain(mode, 'PermittedAi')
          }}
          onDismiss={() => {
            setPendingScreenAssistMode(null)
            setConfirmationDismissed(true)
            void dismissConfirmation()
          }}
        />
      )}

      {settingsOpen && (
        <FloatingSettingsModal
          value={appearance}
          onChange={handleAppearanceChange}
          onClose={() => setSettingsOpen(false)}
        />
      )}
    </main>
  )
}

function SystemLine({ text }: { text: string }) {
  return (
    <div className="flex gap-3 font-mono text-sm text-on-surface-variant/45">
      <span className="text-primary/60">&gt;</span>
      <span>{text}</span>
    </div>
  )
}
