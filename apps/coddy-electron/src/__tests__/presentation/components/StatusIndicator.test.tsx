// __tests__/presentation/components/StatusIndicator.test.tsx
import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { StatusIndicator } from '@/presentation/components/StatusIndicator'

describe('StatusIndicator', () => {
  it('shows "Ready" for Idle status', () => {
    render(<StatusIndicator status="Idle" />)
    expect(screen.getByText('Ready')).toBeInTheDocument()
  })

  it('shows "Thinking..." for Thinking', () => {
    render(<StatusIndicator status="Thinking" />)
    expect(screen.getByText('Thinking...')).toBeInTheDocument()
  })

  it('shows "Generating..." for Streaming', () => {
    render(<StatusIndicator status="Streaming" />)
    expect(screen.getByText('Generating...')).toBeInTheDocument()
  })

  it('shows "Error" for Error', () => {
    render(<StatusIndicator status="Error" />)
    expect(screen.getByText('Error')).toBeInTheDocument()
  })

  it('shows "Listening..." for Listening with pulse indicator', () => {
    render(<StatusIndicator status="Listening" />)
    const label = screen.getByText('Listening...')
    expect(label).toBeInTheDocument()
  })

  it('renders all known statuses without crashing', () => {
    const statuses = [
      'Idle', 'Listening', 'Transcribing', 'CapturingScreen',
      'BuildingContext', 'Thinking', 'Streaming', 'Speaking',
      'AwaitingConfirmation', 'Error',
    ] as const

    for (const s of statuses) {
      const { unmount } = render(<StatusIndicator status={s} />)
      expect(screen.getByText(
        s === 'Idle' ? 'Ready' :
        s === 'Streaming' ? 'Generating...' :
        s === 'Listening' ? 'Listening...' :
        s === 'Transcribing' ? 'Transcribing...' :
        s === 'CapturingScreen' ? 'Capturing screen' :
        s === 'BuildingContext' ? 'Building context' :
        s === 'Thinking' ? 'Thinking...' :
        s === 'Speaking' ? 'Speaking...' :
        s === 'AwaitingConfirmation' ? 'Confirm' :
        'Error'
      )).toBeInTheDocument()
      unmount()
    }
  })
})