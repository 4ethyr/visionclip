// presentation/views/DesktopApp/DesktopApp.tsx
// Advanced mode: sidebar + conversation + workspace panels.
// Uses the same useSession hook as FloatingTerminal.

import { useCallback, useState } from 'react'
import type { FloatingAppearanceSettings } from '@/application'
import { loadSettings, saveSettings } from '@/application'
import { useSessionContext } from '@/presentation/hooks'
import { Sidebar, type DesktopTab } from '@/presentation/components/Sidebar'
import { ConversationPanel } from '@/presentation/components/ConversationPanel'
import { WorkspacePanel } from '@/presentation/components/WorkspacePanel'
import { ModelSelector } from '@/presentation/components/ModelSelector'
import { StatusIndicator } from '@/presentation/components/StatusIndicator'
import { FloatingSettingsModal } from '@/presentation/components/FloatingSettingsModal'
import { Icon } from '@/presentation/components/Icon'

export function DesktopApp() {
  const { session, connecting, error, ask, selectModel, openUi } =
    useSessionContext()
  const [activeTab, setActiveTab] = useState<DesktopTab>('chat')
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [appearance, setAppearance] = useState<FloatingAppearanceSettings>(
    () => loadSettings().floatingAppearance,
  )

  const handleClose = useCallback(() => {
    if (typeof window !== 'undefined' && window.replApi) {
      void window.replApi.invoke('window:close')
    }
  }, [])

  const handleMinimize = useCallback(() => {
    if (typeof window !== 'undefined' && window.replApi) {
      void window.replApi.invoke('window:minimize')
    }
  }, [])

  const handleMaximize = useCallback(() => {
    if (typeof window !== 'undefined' && window.replApi) {
      void window.replApi.invoke('window:maximize')
    }
  }, [])

  const handleAppearanceChange = useCallback(
    (next: FloatingAppearanceSettings) => {
      setAppearance(next)
      saveSettings({ floatingAppearance: next })
    },
    [],
  )

  return (
    <div className="desktop-shell flex h-screen overflow-hidden bg-background text-on-surface">
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

      <main className="relative flex min-w-0 flex-1 flex-col">
        <header className="desktop-topbar flex h-12 flex-shrink-0 items-center justify-between border-b border-white/10 bg-zinc-950/60 px-4 backdrop-blur-lg sm:px-6">
          <div className="flex min-w-0 items-center gap-4">
            <span className="font-display text-lg font-bold uppercase tracking-tight text-primary drop-shadow-[0_0_8px_rgba(0,240,255,0.5)]">
              AETHER_CORE
            </span>
            <div className="hidden items-center gap-2 rounded border border-white/5 bg-surface-container/50 px-3 py-1 md:flex">
              <span className="h-2 w-2 rounded-full bg-primary shadow-[0_0_8px_rgba(0,219,233,0.8)]" />
              <span className="font-mono text-[10px] uppercase tracking-[0.18em] text-zinc-400">
                {session.selected_model.name}
              </span>
            </div>
          </div>

          <div className="flex items-center gap-3">
            <StatusIndicator status={session.status} />
            <button
              type="button"
              onClick={() => setActiveTab('settings')}
              className="text-zinc-500 transition-colors hover:text-primary"
              title="Config"
              aria-label="Open config"
            >
              <Icon name="settings" className="h-5 w-5" />
            </button>
            <button
              type="button"
              onClick={() => {
                void openUi('FloatingTerminal')
              }}
              className="text-zinc-500 transition-colors hover:text-primary"
              title="Floating terminal"
              aria-label="Switch to floating terminal"
            >
              <Icon name="terminal" className="h-5 w-5" />
            </button>
            <div className="ml-1 flex items-center gap-1">
              <WindowButton
                label="Minimize"
                icon="minimize"
                onClick={handleMinimize}
              />
              <WindowButton
                label="Maximize"
                icon="maximize"
                onClick={handleMaximize}
              />
              <WindowButton
                label="Close"
                icon="close"
                tone="danger"
                onClick={handleClose}
              />
            </div>
          </div>
        </header>

        {error && (
          <div className="border-b border-red-500/20 bg-red-500/10 px-5 py-2 font-mono text-sm text-red-300">
            {error}
          </div>
        )}

        {connecting && (
          <div className="flex items-center gap-2 border-b border-primary/10 px-5 py-2 font-mono text-sm text-on-surface-variant">
            <span className="h-2 w-2 animate-pulse rounded-full bg-primary" />
            Connecting to daemon...
          </div>
        )}

        <div className="relative min-h-0 flex-1 overflow-hidden">
          {activeTab === 'chat' && (
            <ConversationPanel session={session} onSend={ask} />
          )}

          {activeTab === 'workspace' && (
            <WorkspacePanel items={session.workspace_context} />
          )}

          {activeTab === 'models' && (
            <ModelsTab
              model={session.selected_model}
              onSelect={(model) => {
                void selectModel(model, 'Chat')
              }}
            />
          )}

          {activeTab === 'settings' && (
            <SettingsTab
              appearance={appearance}
              onOpenAppearance={() => setSettingsOpen(true)}
            />
          )}
        </div>
      </main>

      {settingsOpen && (
        <FloatingSettingsModal
          value={appearance}
          onChange={handleAppearanceChange}
          onClose={() => setSettingsOpen(false)}
        />
      )}
    </div>
  )
}

type WindowButtonProps = {
  label: string
  icon: 'close' | 'minimize' | 'maximize'
  tone?: 'default' | 'danger'
  onClick: () => void
}

function WindowButton({
  label,
  icon,
  tone = 'default',
  onClick,
}: WindowButtonProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex h-5 w-5 items-center justify-center rounded-full transition-colors ${
        tone === 'danger'
          ? 'bg-red-500/10 text-red-300/70 hover:text-red-300'
          : 'bg-surface-container-high/70 text-on-surface-variant/70 hover:text-on-surface'
      }`}
      title={label}
      aria-label={label}
    >
      <Icon name={icon} className="h-3.5 w-3.5" />
    </button>
  )
}

function ModelsTab({
  model,
  onSelect,
}: {
  model: { provider: string; name: string }
  onSelect: (model: { provider: string; name: string }) => void
}) {
  return (
    <div className="h-full overflow-y-auto p-5 sm:p-8">
      <div className="mx-auto flex max-w-6xl flex-col gap-6">
        <section className="flex flex-col justify-between gap-4 sm:flex-row sm:items-end">
          <div>
            <p className="mb-2 font-display text-[11px] uppercase tracking-[0.24em] text-primary/80">
              Daemon Pipeline
            </p>
            <h1 className="font-display text-3xl font-semibold tracking-tight text-on-surface">
              Model routing
            </h1>
            <p className="mt-2 max-w-2xl text-sm leading-6 text-on-surface-variant">
              Controle o modelo local usado pelo chat do Coddy. O daemon
              sincroniza a sessão por eventos para manter terminal flutuante e
              desktop alinhados.
            </p>
          </div>
          <ModelSelector model={model} onSelect={onSelect} />
        </section>

        <div className="grid gap-4 md:grid-cols-3">
          <MetricCard label="CPU CORE" value="Nominal" icon="cpu" tone="primary" />
          <MetricCard label="PIPELINE" value="Local" icon="sensors" tone="secondary" />
          <MetricCard label="BACKEND" value={model.provider} icon="cloud" tone="neutral" />
        </div>

        <section className="desktop-glass-panel overflow-hidden rounded-xl">
          <div className="flex items-center justify-between border-b border-white/10 px-5 py-4">
            <div>
              <h2 className="font-display text-lg font-medium text-on-surface">
                Active daemon
              </h2>
              <p className="mt-1 font-mono text-xs text-on-surface-variant/70">
                TCP socket via Ollama · session scoped model
              </p>
            </div>
            <span className="rounded border border-secondary/30 px-2 py-1 font-mono text-[10px] uppercase tracking-[0.18em] text-secondary">
              chat
            </span>
          </div>
          <div className="flex flex-col gap-3 p-4">
            <div className="flex items-center justify-between rounded-lg border border-white/5 bg-surface-container-low/70 p-4">
              <div className="flex items-center gap-4">
                <span className="h-2 w-2 rounded-full bg-primary shadow-[0_0_8px_rgba(0,219,233,0.8)]" />
                <div>
                  <p className="font-display text-base text-on-surface">
                    {model.name}
                  </p>
                  <p className="mt-1 font-mono text-xs text-on-surface-variant/60">
                    provider={model.provider}; keep_alive=15m
                  </p>
                </div>
              </div>
              <span className="font-mono text-[10px] uppercase tracking-[0.18em] text-primary">
                active
              </span>
            </div>
          </div>
        </section>
      </div>
    </div>
  )
}

function SettingsTab({
  appearance,
  onOpenAppearance,
}: {
  appearance: FloatingAppearanceSettings
  onOpenAppearance: () => void
}) {
  return (
    <div className="h-full overflow-y-auto p-5 sm:p-8">
      <div className="mx-auto max-w-5xl">
        <p className="mb-2 font-display text-[11px] uppercase tracking-[0.24em] text-primary/80">
          System Configuration
        </p>
        <h1 className="font-display text-3xl font-semibold tracking-tight text-on-surface">
          Interface controls
        </h1>
        <p className="mt-2 max-w-2xl text-sm leading-6 text-on-surface-variant">
          Ajuste aparência, transparência e glassmorphism do terminal flutuante
          sem reiniciar a sessão.
        </p>

        <section className="desktop-glass-panel mt-6 rounded-xl p-5">
          <div className="flex flex-col justify-between gap-5 md:flex-row md:items-center">
            <div>
              <h2 className="font-display text-lg text-on-surface">
                Floating terminal appearance
              </h2>
              <div className="mt-4 grid gap-3 font-mono text-xs text-on-surface-variant sm:grid-cols-2">
                <span>blur={appearance.blurPx}px</span>
                <span>opacity={Math.round(appearance.transparency * 100)}%</span>
                <span>glass={Math.round(appearance.glassIntensity * 100)}%</span>
                <span>accent={appearance.accentColor}</span>
              </div>
            </div>
            <button
              type="button"
              onClick={onOpenAppearance}
              className="rounded border border-primary/40 px-4 py-2 font-display text-[11px] uppercase tracking-[0.18em] text-primary transition-colors hover:bg-primary/10"
            >
              Open controls
            </button>
          </div>
        </section>
      </div>
    </div>
  )
}

function MetricCard({
  label,
  value,
  icon,
  tone,
}: {
  label: string
  value: string
  icon: 'cloud' | 'cpu' | 'sensors'
  tone: 'primary' | 'secondary' | 'neutral'
}) {
  const color =
    tone === 'primary'
      ? 'text-primary'
      : tone === 'secondary'
        ? 'text-secondary'
        : 'text-on-surface'

  return (
    <div className="desktop-glass-panel flex items-center gap-4 rounded-xl p-5">
      <span className={`flex h-12 w-12 items-center justify-center rounded-full border border-white/10 bg-surface-container-high/70 ${color}`}>
        <Icon name={icon} className="h-5 w-5" />
      </span>
      <div>
        <p className="font-display text-[10px] uppercase tracking-[0.2em] text-on-surface-variant">
          {label}
        </p>
        <p className="mt-1 font-mono text-sm text-on-surface">{value}</p>
      </div>
    </div>
  )
}
