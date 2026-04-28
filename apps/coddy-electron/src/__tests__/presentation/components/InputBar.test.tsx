// __tests__/presentation/components/InputBar.test.tsx
import { describe, it, expect, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { InputBar } from '@/presentation/components/InputBar'

describe('InputBar', () => {
  it('renders textarea with placeholder', () => {
    render(<InputBar onSend={() => {}} />)

    const textarea = screen.getByPlaceholderText('Enter command or prompt...')
    expect(textarea).toBeInTheDocument()
    expect(tagName(textarea)).toBe('TEXTAREA')
    expect(textarea).not.toHaveFocus()
  })

  it('calls onSend with trimmed text on Enter', async () => {
    const onSend = vi.fn()
    render(<InputBar onSend={onSend} />)

    const textarea = screen.getByPlaceholderText('Enter command or prompt...')
    await userEvent.type(textarea, 'hello world{Enter}')

    expect(onSend).toHaveBeenCalledTimes(1)
    expect(onSend).toHaveBeenCalledWith('hello world')
  })

  it('does not call onSend on Shift+Enter (newline only)', async () => {
    const onSend = vi.fn()
    render(<InputBar onSend={onSend} />)

    const textarea = screen.getByPlaceholderText('Enter command or prompt...')
    await userEvent.type(textarea, 'line1{Shift>}{Enter}{/Shift}line2')

    // Type events still fire, but Enter+Shift is NOT a submit
    expect(onSend).not.toHaveBeenCalled()

    // Textarea should contain both lines (user typed Shift+Enter for newline)
    const el = textarea as HTMLTextAreaElement
    expect(el.value).toContain('line1')
    expect(el.value).toContain('line2')
  })

  it('disables textarea when disabled prop is true', () => {
    render(<InputBar onSend={() => {}} disabled />)

    const textarea = screen.getByPlaceholderText('Enter command or prompt...')
    expect(textarea).toBeDisabled()
  })

  it('clears text after successful submit', async () => {
    const onSend = vi.fn()
    render(<InputBar onSend={onSend} />)

    const textarea = screen.getByPlaceholderText('Enter command or prompt...')
    await userEvent.type(textarea, 'submit me{Enter}')

    const el = textarea as HTMLTextAreaElement
    expect(el.value).toBe('')
  })

  it('does not submit empty text', async () => {
    const onSend = vi.fn()
    render(<InputBar onSend={onSend} />)

    const textarea = screen.getByPlaceholderText('Enter command or prompt...')
    await userEvent.type(textarea, '   {Enter}')

    expect(onSend).not.toHaveBeenCalled()
  })
})

function tagName(el: HTMLElement): string {
  return el.tagName
}
