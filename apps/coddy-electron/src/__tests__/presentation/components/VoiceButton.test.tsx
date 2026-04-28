import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { VoiceButton } from '@/presentation/components/VoiceButton'

describe('VoiceButton', () => {
  it('starts one backend voice capture without resubmitting transcript text', async () => {
    const onCapture = vi.fn().mockResolvedValue({
      text: 'Quem foi Rousseau?',
    })

    render(<VoiceButton onCapture={onCapture} />)

    await userEvent.click(screen.getByRole('button', { name: 'Voice input' }))

    expect(onCapture).toHaveBeenCalledOnce()
  })

  it('shows backend capture errors in the button title', async () => {
    const onCapture = vi.fn().mockResolvedValue({
      error: {
        code: 'VOICE_CAPTURE_FAILED',
        message: 'microfone indisponível',
      },
    })

    render(<VoiceButton onCapture={onCapture} />)

    await userEvent.click(screen.getByRole('button', { name: 'Voice input' }))

    expect(
      await screen.findByRole('button', { name: 'Voice input' }),
    ).toHaveAttribute('title', 'microfone indisponível')
  })
})
