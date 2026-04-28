import { describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { FloatingSettingsModal } from '@/presentation/components/FloatingSettingsModal'
import type { FloatingAppearanceSettings } from '@/application'
import { DEFAULT_FLOATING_APPEARANCE } from '@/application'

const VALUE: FloatingAppearanceSettings = {
  blurPx: 24,
  transparency: 0.58,
  glassIntensity: 0.14,
  textColor: '#e5e2e1',
  accentColor: '#00dbe9',
}

describe('FloatingSettingsModal', () => {
  it('renders appearance controls', () => {
    render(
      <FloatingSettingsModal
        value={VALUE}
        onChange={() => {}}
        onClose={() => {}}
      />,
    )

    expect(
      screen.getByRole('dialog', { name: 'Terminal settings' }),
    ).toBeInTheDocument()
    expect(screen.getByLabelText('Blur')).toBeInTheDocument()
    expect(screen.getByLabelText('Transparency')).toBeInTheDocument()
    expect(screen.getByLabelText('Glass effect')).toBeInTheDocument()
    expect(screen.getByLabelText('Text color')).toBeInTheDocument()
    expect(screen.getByLabelText('Accent color')).toBeInTheDocument()
  })

  it('emits normalized appearance changes', () => {
    const onChange = vi.fn()
    render(
      <FloatingSettingsModal
        value={VALUE}
        onChange={onChange}
        onClose={() => {}}
      />,
    )

    fireEvent.change(screen.getByLabelText('Blur'), { target: { value: '36' } })
    fireEvent.change(screen.getByLabelText('Text color'), {
      target: { value: '#ffffff' },
    })

    expect(onChange).toHaveBeenCalledWith({ ...VALUE, blurPx: 36 })
    expect(onChange).toHaveBeenCalledWith({ ...VALUE, textColor: '#ffffff' })
  })

  it('resets to default appearance and closes on demand', async () => {
    const onChange = vi.fn()
    const onClose = vi.fn()
    render(
      <FloatingSettingsModal
        value={VALUE}
        onChange={onChange}
        onClose={onClose}
      />,
    )

    await userEvent.click(screen.getByRole('button', { name: 'Reset' }))
    await userEvent.click(screen.getByRole('button', { name: 'Done' }))

    expect(onChange).toHaveBeenCalledWith(DEFAULT_FLOATING_APPEARANCE)
    expect(onClose).toHaveBeenCalledTimes(1)
  })
})
