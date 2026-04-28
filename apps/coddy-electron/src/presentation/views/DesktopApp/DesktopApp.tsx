// presentation/views/DesktopApp/DesktopApp.tsx
// Advanced mode: sidebar + conversation + workspace panels.
// Uses the same useSession hook as FloatingTerminal.

import { useState } from 'react'
import { useSessionContext } from '@/presentation/hooks'
import { Sidebar, type DesktopTab } from '@/presentation/components/Sidebar'
import { ConversationPanel } from '@/presentation/components/ConversationPanel'
import { WorkspacePanel } from '@/presentation/components/WorkspacePanel'
import { ModelSelector } from '@/presentation/components/ModelSelector'

export function DesktopApp() {
  const { session, connecting, error, ask, selectModel, openUi } =
    useSessionContext()
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
          mode={session.mode}
          onOpenMode={(mode) => {
            void openUi(mode)
          }}
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
          <div className="flex-1 overflow-y-auto px-8 py-7">
            <div className="max-w-2xl">
              <p className="text-xs uppercase tracking-[0.25em] text-primary/70 mb-2">
                Model routing
              </p>
              <h1 className="text-2xl font-semibold text-on-surface mb-3">
                Modelos do Coddy
              </h1>
              <p className="text-sm text-on-surface-variant mb-6">
                Selecione o modelo de chat usado pelo backend. O daemon emite
                o evento ModelSelected e todas as telas sincronizam a sessão.
              </p>

              <div className="rounded-2xl border border-primary/15 bg-surface-container/70 backdrop-blur-xl p-5 flex items-center justify-between gap-4">
                <div>
                  <p className="text-sm font-medium text-on-surface">
                    Modelo de chat ativo
                  </p>
                  <p className="text-xs text-on-surface-variant mt-1">
                    {session.selected_model.provider}/
                    {session.selected_model.name}
                  </p>
                </div>
                <ModelSelector
                  model={session.selected_model}
                  onSelect={(model) => {
                    void selectModel(model, 'Chat')
                  }}
                />
              </div>
            </div>
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
