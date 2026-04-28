import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ModelSelector } from '@/presentation/components/ModelSelector'

describe('ModelSelector', () => {
  it('renders the active model', () => {
    render(
      <ModelSelector model={{ provider: 'ollama', name: 'gemma4-E2B' }} />,
    )

    expect(screen.getAllByText('ollama/gemma4-E2B')).toHaveLength(2)
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
      screen.getByRole('button', { name: 'ollama/qwen2.5:0.5b' }),
    )

    expect(onSelect).toHaveBeenCalledWith({
      provider: 'ollama',
      name: 'qwen2.5:0.5b',
    })
  })
})
