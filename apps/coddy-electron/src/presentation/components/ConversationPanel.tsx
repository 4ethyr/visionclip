// ConversationPanel: chat messages + input bar for DesktopApp.

import { useRef, useEffect } from 'react'
import type { ReplSession } from '@/domain'
import { MessageBubble } from '@/presentation/components/MessageBubble'
import { InputBar } from '@/presentation/components/InputBar'
import { StreamingText } from '@/presentation/components/StreamingText'
import { Icon } from '@/presentation/components/Icon'

interface Props {
  session: ReplSession
  onSend: (text: string) => void
}

export function ConversationPanel({ session, onSend }: Props) {
  const messagesEndRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' })
  }, [session.messages.length, session.streaming_text])

  return (
    <div className="relative flex h-full flex-1 flex-col overflow-hidden">
      <div className="desktop-canvas flex-1 overflow-y-auto px-4 py-6 sm:px-8">
        <div className="mx-auto flex max-w-5xl flex-col gap-7 pb-28">
          <div className="flex justify-center">
            <div className="rounded border border-white/5 bg-surface-container/40 px-4 py-2 text-center backdrop-blur-md">
              <p className="font-mono text-xs uppercase tracking-[0.18em] text-on-surface-variant/45">
                session_initialized // awaiting command
              </p>
            </div>
          </div>

          <PlanOfAttack />

          {session.messages.map((msg) => (
            <MessageBubble key={msg.id} message={msg} />
          ))}

          {session.streaming_text && (
            <div className="flex items-start gap-4">
              <div className="flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-md border border-primary bg-primary/10 text-primary">
                <Icon name="bot" className="h-4 w-4" />
              </div>
              <div className="desktop-glass-panel max-w-3xl flex-1 rounded-lg px-5 py-4">
                <div className="mb-2 font-mono text-[10px] uppercase tracking-[0.22em] text-primary/80">
                  coddy_agent
                </div>
                <p className="whitespace-pre-wrap break-words text-sm leading-6 text-on-surface">
                  <StreamingText text={session.streaming_text} />
                  <span className="streaming-cursor" />
                </p>
              </div>
            </div>
          )}

          <div ref={messagesEndRef} />
        </div>
      </div>

      <div className="pointer-events-none absolute bottom-5 left-0 right-0 z-20 flex justify-center px-4">
        <div className="pointer-events-auto w-full max-w-3xl rounded-full border border-white/15 bg-surface-container/90 p-2 backdrop-blur-2xl">
          <InputBar
            onSend={onSend}
            disabled={
              session.status === 'Streaming' || session.status === 'Thinking'
            }
            placeholder={
              session.status === 'Thinking'
                ? 'Thinking...'
                : 'Instruct Coddy agent...'
            }
          />
        </div>
      </div>
    </div>
  )
}

function PlanOfAttack() {
  return (
    <section className="desktop-glass-panel overflow-hidden rounded-xl">
      <div className="border-b border-white/5 bg-gradient-to-br from-surface-container-high/80 to-transparent p-5">
        <h2 className="mb-4 flex items-center gap-2 font-display text-[11px] uppercase tracking-[0.2em] text-primary">
          <Icon name="sensors" className="h-4 w-4" />
          Plan of attack
        </h2>
        <div className="flex flex-col gap-0 pl-1">
          <TaskStep label="Read command context" state="done" />
          <TaskStep label="Classify intent and risk" state="active" />
          <TaskStep label="Execute safe tool or answer in REPL" state="pending" />
        </div>
      </div>
    </section>
  )
}

function TaskStep({
  label,
  state,
}: {
  label: string
  state: 'done' | 'active' | 'pending'
}) {
  return (
    <div className="relative flex gap-4 pb-3 last:pb-0">
      {state !== 'pending' && (
        <div className="absolute left-[5px] top-4 h-full w-px bg-outline-variant/50" />
      )}
      <span
        className={`z-10 mt-1.5 flex h-3 w-3 shrink-0 items-center justify-center rounded-full border ${
          state === 'active'
            ? 'border-primary bg-surface-dim shadow-[0_0_8px_rgba(0,219,233,0.6)]'
            : 'border-outline-variant bg-surface-dim'
        }`}
      >
        {state !== 'pending' && (
          <span
            className={`h-1.5 w-1.5 rounded-full ${
              state === 'active' ? 'bg-primary' : 'bg-outline'
            }`}
          />
        )}
      </span>
      <p
        className={`font-mono text-sm ${
          state === 'active'
            ? 'font-bold text-primary'
            : state === 'done'
              ? 'text-on-surface-variant/60 line-through'
              : 'text-on-surface-variant/45'
        }`}
      >
        {label}
      </p>
    </div>
  )
}
