import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ModelSelector } from '@/presentation/components/ModelSelector'

describe('ModelSelector', () => {
  it('renders the active model', () => {
    render(
      <ModelSelector model={{ provider: 'ollama', name: 'gemma4-E2B' }} />,
    )

    expect(screen.getByText('MODEL: gemma4-E2B')).toBeInTheDocument()
    expect(
      screen.getByRole('button', { name: 'Active model ollama/gemma4-E2B' }),
    ).toBeInTheDocument()
  })

  it('emits the selected model from the dropdown', async () => {
    const onSelect = vi.fn()
    render(
      <ModelSelector
        model={{ provider: 'ollama', name: 'gemma4-E2B' }}
        onSelect={onSelect}
      />,
    )

    await userEvent.click(
      screen.getByRole('button', { name: /qwen2.5:0.5b/ }),
    )

    expect(onSelect).toHaveBeenCalledWith({
      provider: 'ollama',
      name: 'qwen2.5:0.5b',
    })
  })
})
