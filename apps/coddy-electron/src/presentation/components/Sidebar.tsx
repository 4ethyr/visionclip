// Sidebar for the DesktopApp view.
// Navigation with icons for Chat, Workspace, Models, Settings.

import type { ReplMode, SessionStatus } from '@/domain'
import { StatusIndicator } from '@/presentation/components/StatusIndicator'

export type DesktopTab = 'chat' | 'workspace' | 'models' | 'settings'

interface Props {
  activeTab: DesktopTab
  onTabChange: (tab: DesktopTab) => void
  connected: boolean
  status: SessionStatus
  mode: ReplMode
  onOpenMode: (mode: ReplMode) => void
}

const TABS: { id: DesktopTab; label: string; icon: string }[] = [
  { id: 'chat', label: 'Chat', icon: '💬' },
  { id: 'workspace', label: 'Workspace', icon: '📁' },
  { id: 'models', label: 'Models', icon: '🧠' },
  { id: 'settings', label: 'Settings', icon: '⚙' },
]

export function Sidebar({
  activeTab,
  onTabChange,
  connected,
  status,
  mode,
  onOpenMode,
}: Props) {
  return (
    <aside className="w-56 bg-surface-dim/80 backdrop-blur-xl border-r border-primary/10 flex flex-col flex-shrink-0">
      {/* Logo */}
      <div className="px-5 py-4 border-b border-primary/10">
        <div className="flex items-center gap-2">
          <span className="text-primary text-lg font-mono">&gt;_</span>
          <span className="font-bold text-primary text-sm tracking-wider">
            CODDY
          </span>
        </div>
      </div>

      {/* Connection status */}
      <div className="px-5 py-2 border-b border-primary/10">
        <StatusIndicator status={status} />
      </div>

      {/* Navigation */}
      <nav className="flex-1 flex flex-col gap-0.5 p-2">
        {TABS.map((tab) => (
          <button
            key={tab.id}
            type="button"
            onClick={() => onTabChange(tab.id)}
            className={`flex items-center gap-3 px-3 py-2 rounded-lg text-sm transition-colors ${
              activeTab === tab.id
                ? 'bg-primary/10 text-primary'
                : 'text-on-surface-variant hover:bg-surface-container hover:text-on-surface'
            }`}
          >
            <span className="text-base">{tab.icon}</span>
            <span>{tab.label}</span>
          </button>
        ))}
      </nav>

      {/* Bottom status */}
      <div className="px-5 py-3 border-t border-primary/10 flex flex-col gap-2">
        {mode === 'DesktopApp' && (
          <button
            type="button"
            onClick={() => onOpenMode('FloatingTerminal')}
            className="flex items-center gap-2 text-xs text-on-surface-variant hover:text-on-surface transition-colors"
          >
            <span>⬜</span>
            <span>Switch to floating</span>
          </button>
        )}
        <div className="flex items-center gap-2">
          <span
            className={`w-1.5 h-1.5 rounded-full ${
              connected ? 'bg-green-400' : 'bg-on-surface-variant/40'
            }`}
          />
          <span className="text-xs text-on-surface-variant">
            {connected ? 'Daemon connected' : 'Disconnected'}
          </span>
        </div>
      </div>
    </aside>
  )
}
