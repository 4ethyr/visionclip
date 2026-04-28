// __tests__/presentation/components/MessageBubble.test.tsx
import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { MessageBubble } from '@/presentation/components/MessageBubble'

describe('MessageBubble', () => {
  it('renders user message', () => {
    render(
      <MessageBubble
        message={{ id: '1', role: 'user', text: 'hello from user' }}
      />,
    )

    expect(screen.getByText('hello from user')).toBeInTheDocument()
  })

  it('renders assistant message', () => {
    render(
      <MessageBubble
        message={{ id: '2', role: 'assistant', text: 'hello from ai' }}
      />,
    )

    expect(screen.getByText('hello from ai')).toBeInTheDocument()
  })

  it('renders inline code without blocks', () => {
    const text = 'Here is some `inline code` and text'
    render(
      <MessageBubble
        message={{ id: '3', role: 'assistant', text }}
      />,
    )

    expect(screen.getByText(text)).toBeInTheDocument()
  })

  it('renders code blocks with language label', () => {
    const text = '```rust\nfn main() {}\n```'
    const { container } = render(
      <MessageBubble
        message={{ id: '4', role: 'assistant', text }}
      />,
    )

    // Code is syntax-highlighted into multiple spans — check content exists
    expect(container.textContent).toContain('fn')
    expect(container.textContent).toContain('main()')
    // Language label is shown lowercase
    expect(screen.getByText('rust')).toBeInTheDocument()
    // Copy button exists
    expect(screen.getByText('Copy')).toBeInTheDocument()
  })

  it('renders a user command icon without emoji avatars', () => {
    render(
      <MessageBubble
        message={{ id: '5', role: 'user', text: 'hi' }}
      />,
    )

    expect(screen.getByTestId('user-message-icon')).toBeInTheDocument()
    expect(screen.queryByText('👤')).not.toBeInTheDocument()
  })

  it('renders an assistant agent icon without emoji avatars', () => {
    render(
      <MessageBubble
        message={{ id: '6', role: 'assistant', text: 'hello' }}
      />,
    )

    expect(screen.getByTestId('assistant-message-icon')).toBeInTheDocument()
    expect(screen.queryByText('🤖')).not.toBeInTheDocument()
  })
})
