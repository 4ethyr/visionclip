// presentation/components/MessageBubble.tsx
// Renders one REPL transcript entry, not a chat bubble.

import type { ReplMessage } from '@/domain'
import type { JSX } from 'react'
import { CodeBlock, parseMarkdown } from './CodeBlock'
import { Icon } from './Icon'

interface Props {
  message: ReplMessage
}

export function MessageBubble({ message }: Props) {
  const isUser = message.role === 'user'

  if (isUser) {
    return (
      <div className="group flex w-full items-start gap-3 font-mono text-sm">
        <span className="mt-0.5 text-primary drop-shadow-[0_0_8px_rgba(0,219,233,0.65)]">
          &gt;
        </span>
        <div className="min-w-0 flex-1">
          <div className="mb-1 flex items-center gap-2 text-[10px] uppercase tracking-[0.22em] text-on-surface-variant/45">
            <Icon
              name="user"
              className="h-3.5 w-3.5"
              data-testid="user-message-icon"
            />
            user_input
          </div>
          <p className="break-words text-on-surface/95">{message.text}</p>
        </div>
      </div>
    )
  }

  return (
    <div className="flex w-full items-start gap-4">
      <div className="mt-1 flex h-8 w-8 flex-shrink-0 items-center justify-center rounded-md border border-primary bg-primary/10 text-primary shadow-[0_0_22px_rgba(0,219,233,0.2)]">
        <Icon
          name="bot"
          className="h-4 w-4"
          data-testid="assistant-message-icon"
        />
      </div>

      <div className="min-w-0 flex-1 rounded-lg border border-outline-variant/70 bg-surface-container/45 px-5 py-4 shadow-[inset_0_1px_0_rgba(255,255,255,0.03)]">
        <div className="mb-2 font-mono text-[10px] uppercase tracking-[0.22em] text-primary/80">
          coddy_agent
        </div>
        {renderContent(message.text)}
      </div>
    </div>
  )
}

function renderContent(text: string): JSX.Element {
  const segments = parseMarkdown(text)

  if (segments.length === 1 && segments[0]?.type === 'text') {
    return (
      <p className="text-sm text-on-surface whitespace-pre-wrap break-words">
        {segments[0].content}
      </p>
    )
  }

  return (
    <div className="flex flex-col gap-3">
      {segments.map((seg, i) =>
        seg.type === 'code' ? (
          <CodeBlock key={i} code={seg.content} language={seg.language} />
        ) : (
          <p key={i} className="text-sm leading-6 text-on-surface whitespace-pre-wrap break-words">
            {seg.content}
          </p>
        ),
      )}
    </div>
  )
}
