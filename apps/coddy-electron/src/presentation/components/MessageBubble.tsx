// presentation/components/MessageBubble.tsx
// Renders a single chat message (user or assistant).

import type { ReplMessage } from '@/domain'
import type { JSX } from 'react'
import { CodeBlock, parseMarkdown } from './CodeBlock'

interface Props {
  message: ReplMessage
}

export function MessageBubble({ message }: Props) {
  const isUser = message.role === 'user'

  return (
    <div
      className={`flex gap-3 items-start max-w-3xl ${
        isUser ? 'self-end flex-row-reverse' : ''
      }`}
    >
      {/* Avatar */}
      <div
        className={`w-8 h-8 rounded-full flex items-center justify-center border flex-shrink-0 ${
          isUser
            ? 'bg-surface-container-high border-outline-variant'
            : 'bg-primary/10 border-primary/30'
        }`}
      >
        <span className="text-sm">
          {isUser ? '👤' : '🤖'}
        </span>
      </div>

      {/* Content */}
      <div
        className={`px-4 py-3 rounded-xl border max-w-[80%] ${
          isUser
            ? 'bg-surface-container-low border-outline-variant rounded-tr-none'
            : 'bg-surface-container border-primary/10 rounded-tl-none'
        }`}
      >
        {isUser ? (
          <p className="text-sm text-on-surface whitespace-pre-wrap break-words">
            {message.text}
          </p>
        ) : (
          renderContent(message.text)
        )}
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
    <div className="flex flex-col gap-2">
      {segments.map((seg, i) =>
        seg.type === 'code' ? (
          <CodeBlock key={i} code={seg.content} language={seg.language} />
        ) : (
          <p key={i} className="text-sm text-on-surface whitespace-pre-wrap break-words">
            {seg.content}
          </p>
        ),
      )}
    </div>
  )
}