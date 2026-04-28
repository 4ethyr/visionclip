import { describe, expect, it, vi } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { Sidebar } from '@/presentation/components/Sidebar'

describe('Sidebar', () => {
  it('renders desktop navigation with svg icons instead of emoji labels', () => {
    render(
      <Sidebar
        activeTab="chat"
        onTabChange={() => {}}
        connected
        status="Idle"
        mode="DesktopApp"
        onOpenMode={() => {}}
      />,
    )

    expect(screen.getByText('SYSTEM_REPL')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Terminal/ })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Workspace/ })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Neural_Link/ })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Config/ })).toBeInTheDocument()
    for (const emoji of ['💬', '📁', '🧠', '⚙']) {
      expect(screen.queryByText(emoji)).not.toBeInTheDocument()
    }
  })

  it('emits tab and floating mode changes', async () => {
    const onTabChange = vi.fn()
    const onOpenMode = vi.fn()
    render(
      <Sidebar
        activeTab="chat"
        onTabChange={onTabChange}
        connected
        status="Idle"
        mode="DesktopApp"
        onOpenMode={onOpenMode}
      />,
    )

    await userEvent.click(screen.getByRole('button', { name: /Workspace/ }))
    await userEvent.click(
      screen.getByRole('button', { name: /Switch to floating/ }),
    )

    expect(onTabChange).toHaveBeenCalledWith('workspace')
    expect(onOpenMode).toHaveBeenCalledWith('FloatingTerminal')
  })
})
