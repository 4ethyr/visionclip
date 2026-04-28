import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { AssessmentConfirmModal } from '@/presentation/components/AssessmentConfirmModal'

vi.mock('@/presentation/hooks', () => ({
  useSessionContext: () => ({
    session: {
      policy: 'UnknownAssessment',
    },
  }),
}))

describe('AssessmentConfirmModal', () => {
  it('calls the structured confirm handler', async () => {
    const onConfirm = vi.fn()
    const onDismiss = vi.fn()

    render(
      <AssessmentConfirmModal
        onConfirm={onConfirm}
        onDismiss={onDismiss}
      />,
    )

    await userEvent.click(screen.getByRole('button', {
      name: 'Confirm & Proceed',
    }))

    expect(onConfirm).toHaveBeenCalledOnce()
    expect(onDismiss).not.toHaveBeenCalled()
  })

  it('calls dismiss without submitting prompt text', async () => {
    const onConfirm = vi.fn()
    const onDismiss = vi.fn()

    render(
      <AssessmentConfirmModal
        onConfirm={onConfirm}
        onDismiss={onDismiss}
      />,
    )

    await userEvent.click(screen.getByRole('button', { name: 'Dismiss' }))

    expect(onDismiss).toHaveBeenCalledOnce()
    expect(onConfirm).not.toHaveBeenCalled()
  })
})
