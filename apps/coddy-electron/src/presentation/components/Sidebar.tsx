// Sidebar for the DesktopApp view.
// Navigation with icons for Chat, Workspace, Models, Settings.

import type { ReplMode, SessionStatus } from '@/domain'
import { StatusIndicator } from '@/presentation/components/StatusIndicator'
import { Icon, type IconName } from '@/presentation/components/Icon'

export type DesktopTab = 'chat' | 'workspace' | 'models' | 'settings'

interface Props {
  activeTab: DesktopTab
  onTabChange: (tab: DesktopTab) => void
  connected: boolean
  status: SessionStatus
  mode: ReplMode
  onOpenMode: (mode: ReplMode) => void
}

const TABS: { id: DesktopTab; label: string; icon: IconName }[] = [
  { id: 'chat', label: 'Terminal', icon: 'terminal' },
  { id: 'workspace', label: 'Workspace', icon: 'file' },
  { id: 'models', label: 'Neural_Link', icon: 'cpu' },
  { id: 'settings', label: 'Config', icon: 'settings' },
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
    <aside className="desktop-sidebar hidden w-[240px] flex-shrink-0 flex-col border-r border-white/5 bg-zinc-950/40 backdrop-blur-2xl md:flex">
      <div className="border-b border-white/5 px-6 py-6">
        <div className="flex items-center gap-3">
          <span className="flex h-9 w-9 items-center justify-center rounded-full border border-primary/50 bg-primary/10 text-primary shadow-[0_0_22px_rgba(0,219,233,0.18)]">
            <Icon name="bot" className="h-4 w-4" />
          </span>
          <div className="min-w-0">
            <h1 className="font-display text-sm font-black uppercase tracking-tight text-primary">
              SYSTEM_REPL
            </h1>
            <p className="mt-1 font-mono text-[10px] uppercase tracking-[0.2em] text-on-surface-variant/60">
              V_4.0.2_ACTIVE
            </p>
          </div>
        </div>
      </div>

      <div className="border-b border-white/5 px-5 py-3">
        <StatusIndicator status={status} />
      </div>

      <nav className="flex flex-1 flex-col gap-1 px-2 py-4">
        {TABS.map((tab) => (
          <button
            key={tab.id}
            type="button"
            onClick={() => onTabChange(tab.id)}
            className={`group flex items-center gap-3 border-r-2 px-4 py-3 text-left font-display text-[10px] uppercase tracking-[0.18em] transition-all ${
              activeTab === tab.id
                ? 'border-primary bg-primary/5 text-primary'
                : 'border-transparent text-zinc-600 hover:bg-white/5 hover:text-zinc-200'
            }`}
          >
            <Icon
              name={tab.icon}
              className="h-[18px] w-[18px] transition-transform group-hover:scale-105"
            />
            <span>{tab.label}</span>
          </button>
        ))}
      </nav>

      <div className="flex flex-col gap-4 border-t border-white/5 px-5 py-5">
        {mode === 'DesktopApp' && (
          <button
            type="button"
            onClick={() => onOpenMode('FloatingTerminal')}
            className="flex items-center gap-3 font-display text-[10px] uppercase tracking-[0.18em] text-zinc-500 transition-colors hover:text-primary"
          >
            <Icon name="desktop" className="h-4 w-4" />
            <span>Switch to floating</span>
          </button>
        )}
        <div className="flex items-center gap-2 font-mono">
          <span
            className={`h-1.5 w-1.5 rounded-full ${
              connected
                ? 'bg-primary shadow-[0_0_8px_rgba(0,219,233,0.8)]'
                : 'bg-on-surface-variant/40'
            }`}
          />
          <span className="text-[11px] text-on-surface-variant/70">
            {connected ? 'Daemon connected' : 'Disconnected'}
          </span>
        </div>
      </div>
    </aside>
  )
}
