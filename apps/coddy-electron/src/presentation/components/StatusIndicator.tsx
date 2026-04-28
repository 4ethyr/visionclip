// presentation/components/StatusIndicator.tsx
// Shows the current session status as a pill with color coding.

import type { SessionStatus } from '@/domain'

const STATUS_LABELS: Record<SessionStatus, string> = {
  Idle: 'Ready',
  Listening: 'Listening...',
  Transcribing: 'Transcribing...',
  CapturingScreen: 'Capturing screen',
  BuildingContext: 'Building context',
  Thinking: 'Thinking...',
  Streaming: 'Generating...',
  Speaking: 'Speaking...',
  AwaitingConfirmation: 'Confirm',
  Error: 'Error',
}

const STATUS_COLORS: Record<SessionStatus, string> = {
  Idle: 'bg-primary/10 text-primary border-primary/20',
  Listening: 'bg-yellow-500/10 text-yellow-400 border-yellow-500/20',
  Transcribing: 'bg-yellow-500/10 text-yellow-400 border-yellow-500/20',
  CapturingScreen: 'bg-blue-500/10 text-blue-400 border-blue-500/20',
  BuildingContext: 'bg-purple-500/10 text-purple-400 border-purple-500/20',
  Thinking: 'bg-cyan-500/10 text-cyan-400 border-cyan-500/20 animate-pulse',
  Streaming: 'bg-green-500/10 text-green-400 border-green-500/20',
  Speaking: 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20',
  AwaitingConfirmation: 'bg-orange-500/10 text-orange-400 border-orange-500/20',
  Error: 'bg-red-500/10 text-red-400 border-red-500/20',
}

interface Props {
  status: SessionStatus
}

export function StatusIndicator({ status }: Props) {
  const label = STATUS_LABELS[status] ?? status
  const color = STATUS_COLORS[status] ?? 'bg-surface text-on-surface-variant'

  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 font-mono text-xs ${color}`}
    >
      {status !== 'Idle' && status !== 'Error' && (
        <span className="w-1.5 h-1.5 rounded-full bg-current animate-pulse" />
      )}
      {label}
    </span>
  )
}
