// Workspace panel: shows context items (files, screen captures, etc.)

import type { ContextItem } from '@/domain'
import { Icon } from './Icon'

interface Props {
  items: ContextItem[]
}

export function WorkspacePanel({ items }: Props) {
  return (
    <div className="h-full overflow-y-auto p-5 sm:p-8">
      <div className="mx-auto flex max-w-6xl flex-col gap-6">
        <header className="flex flex-col justify-between gap-4 sm:flex-row sm:items-end">
          <div>
            <p className="mb-2 font-display text-[11px] uppercase tracking-[0.24em] text-primary/80">
              Context Workspace
            </p>
            <h1 className="font-display text-3xl font-semibold tracking-tight text-on-surface">
              Active workspace
            </h1>
            <p className="mt-2 max-w-2xl text-sm leading-6 text-on-surface-variant">
              Arquivos, capturas e documentos que alimentam a sessão local do
              agente.
            </p>
          </div>
          <button
            type="button"
            className="desktop-glass-panel inline-flex items-center gap-2 rounded-lg px-4 py-2 font-display text-[11px] uppercase tracking-[0.18em] text-primary transition-colors hover:bg-primary/10"
          >
            <Icon name="cloud" className="h-4 w-4" />
            Local environment
          </button>
        </header>

        <section className="desktop-dropzone relative flex min-h-[340px] flex-col items-center justify-center overflow-hidden rounded-xl border-2 border-dashed border-outline-variant/70 p-6 text-center transition-colors hover:border-primary/50">
          <div className="pointer-events-none absolute -right-24 -top-24 h-64 w-64 rounded-full bg-secondary-container/20 blur-[80px]" />
          <div className="mb-6 flex h-20 w-20 items-center justify-center rounded-full border border-white/10 bg-surface-container-highest text-primary transition-transform duration-500 group-hover:scale-110">
            <Icon name="file" className="h-9 w-9 drop-shadow-[0_0_12px_rgba(0,219,233,0.6)]" />
          </div>
          <h2 className="font-display text-2xl font-medium text-on-surface">
            Drop context files here
          </h2>
          <p className="mt-3 max-w-md text-sm leading-6 text-on-surface-variant">
            Injete documentos, trechos de código ou screenshots no contexto
            ativo do Coddy. O backend ainda vai receber o upload real.
          </p>
          <button
            type="button"
            className="mt-6 rounded border border-primary/60 px-5 py-2 font-display text-[11px] uppercase tracking-[0.18em] text-primary transition-all hover:bg-primary/10"
          >
            Browse files
          </button>
        </section>

        <section className="flex flex-wrap gap-3">
          {items.length === 0 ? (
            <ContextPill label="No context items yet" muted />
          ) : (
            items.map((item) => (
              <ContextPill
                key={item.id}
                label={item.label}
                sensitive={item.sensitive}
              />
            ))
          )}
        </section>
      </div>
    </div>
  )
}

function ContextPill({
  label,
  sensitive = false,
  muted = false,
}: {
  label: string
  sensitive?: boolean
  muted?: boolean
}) {
  return (
    <div
      className={`flex items-center gap-2 rounded-full border px-3 py-2 font-mono text-sm ${
        sensitive
          ? 'border-yellow-500/20 bg-yellow-500/5 text-yellow-200'
          : muted
            ? 'border-outline/10 bg-surface-container-highest/40 text-on-surface-variant/50'
            : 'border-outline/20 bg-surface-container-highest text-on-surface'
      }`}
    >
      <Icon
        name={sensitive ? 'lock' : 'file'}
        className={`h-4 w-4 ${sensitive ? 'text-yellow-300' : 'text-primary'}`}
      />
      <span className="max-w-[220px] truncate">{label}</span>
    </div>
  )
}
