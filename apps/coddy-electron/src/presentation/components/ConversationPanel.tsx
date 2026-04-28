// ConversationPanel: chat messages + input bar for DesktopApp.

import { useRef, useEffect } from 'react'
import type { ReplSession } from '@/domain'
import { MessageBubble } from '@/presentation/components/MessageBubble'
import { InputBar } from '@/presentation/components/InputBar'
import { StreamingText } from '@/presentation/components/StreamingText'

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
    <div className="flex-1 flex flex-col h-full">
      {/* Messages */}
      <div className="flex-1 overflow-y-auto px-6 py-4 flex flex-col gap-4">
        <div className="text-xs text-on-surface-variant/40 text-center py-4">
          Start a conversation below
        </div>

        {session.messages.map((msg) => (
          <MessageBubble key={msg.id} message={msg} />
        ))}

        {/* Streaming indicator */}
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

      {/* Input */}
      <div className="px-6 py-4 border-t border-primary/10 bg-background/60">
        <InputBar
          onSend={onSend}
          disabled={
            session.status === 'Streaming' || session.status === 'Thinking'
          }
          placeholder="Ask anything..."
        />
      </div>
    </div>
  )
}