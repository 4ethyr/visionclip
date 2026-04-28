// presentation/views/DesktopApp/DesktopApp.tsx
// Advanced mode: sidebar + conversation + workspace panels.
// Uses the same useSession hook as FloatingTerminal.

import { useState } from 'react'
import { useSessionContext } from '@/presentation/hooks'
import { Sidebar, type DesktopTab } from '@/presentation/components/Sidebar'
import { ConversationPanel } from '@/presentation/components/ConversationPanel'
import { WorkspacePanel } from '@/presentation/components/WorkspacePanel'

export function DesktopApp() {
  const { session, connecting, error, ask } = useSessionContext()
  const [activeTab, setActiveTab] = useState<DesktopTab>('chat')
  const [sidebarOpen] = useState(true)

  return (
    <div className="h-screen flex bg-background">
      {/* Sidebar */}
      {sidebarOpen && (
        <Sidebar
          activeTab={activeTab}
          onTabChange={setActiveTab}
          connected={!connecting}
          status={session.status}
        />
      )}

      {/* Main content area */}
      <div className="flex-1 flex flex-col min-w-0">
        {error && (
          <div className="bg-red-500/10 border-b border-red-500/20 px-5 py-2 text-sm text-red-400">
            {error}
          </div>
        )}

        {connecting && (
          <div className="flex items-center gap-2 text-on-surface-variant text-sm px-5 py-2 border-b border-primary/10">
            <span className="animate-pulse">●</span>
            Connecting to daemon...
          </div>
        )}

        {activeTab === 'chat' && (
          <ConversationPanel session={session} onSend={ask} />
        )}

        {activeTab === 'workspace' && (
          <WorkspacePanel items={session.workspace_context} />
        )}

        {activeTab === 'models' && (
          <div className="flex-1 flex items-center justify-center text-on-surface-variant text-sm">
            Model management — coming soon.
          </div>
        )}

        {activeTab === 'settings' && (
          <div className="flex-1 flex items-center justify-center text-on-surface-variant text-sm">
            Settings — coming soon.
          </div>
        )}
      </div>
    </div>
  )
}