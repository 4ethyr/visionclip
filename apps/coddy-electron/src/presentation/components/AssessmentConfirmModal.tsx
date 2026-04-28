// Modal shown when the policy evaluator requires confirmation (UnknownAssessment).
// Mirrors the AssistancePolicy workflow from domain/reducers/policyEvaluator.ts.

import { useEffect } from 'react'
import { useSessionContext } from '@/presentation/hooks'
import { Icon } from './Icon'

interface Props {
  onConfirm: () => void
  onDismiss: () => void
}

export function AssessmentConfirmModal({ onConfirm, onDismiss }: Props) {
  const { session } = useSessionContext()

  // Keyboard: Enter confirms, Escape dismisses
  useEffect(() => {
    const handler = (e: globalThis.KeyboardEvent) => {
      if (e.key === 'Escape') onDismiss()
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [onDismiss])

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 backdrop-blur-sm">
      <div className="bg-surface-container-high border border-outline-variant rounded-xl p-6 max-w-md w-full mx-4 shadow-2xl">
        <div className="mx-auto mb-4 flex h-10 w-10 items-center justify-center rounded-full border border-yellow-400/30 bg-yellow-500/15 text-yellow-300">
          <Icon name="lock" className="h-5 w-5" />
        </div>

        {/* Title */}
        <h2 className="text-base font-semibold text-on-surface text-center mb-2">
          Assessment Policy Confirmation
        </h2>

        {/* Description */}
        <p className="text-sm text-on-surface-variant text-center mb-1 leading-relaxed">
          AI assistance for this content requires your permission. What kind of
          help are you working on?
        </p>

        <p className="text-xs text-on-surface-variant/60 text-center mb-5">
          Current policy: <code className="text-primary">{session.policy}</code>
        </p>

        {/* Actions */}
        <div className="flex gap-3">
          <button
            type="button"
            onClick={onDismiss}
            className="flex-1 px-4 py-2 rounded-lg border border-outline-variant text-sm text-on-surface-variant hover:text-on-surface hover:bg-surface-container transition-colors"
          >
            Dismiss
          </button>
          <button
            type="button"
            onClick={onConfirm}
            className="flex-1 px-4 py-2 rounded-lg bg-primary/20 border border-primary/30 text-sm text-primary hover:bg-primary/30 transition-colors"
          >
            Confirm & Proceed
          </button>
        </div>
      </div>
    </div>
  )
}
