// Model selector: shows current model and allows switching.
// Stub for now — model list comes from the daemon later.

import type { ModelRef } from '@/domain'
import { Icon } from './Icon'

interface Props {
  model: ModelRef
  onSelect?: (model: ModelRef) => void
}

const SUGGESTED_MODELS: ModelRef[] = [
  { provider: 'ollama', name: 'gemma4-E2B' },
  { provider: 'ollama', name: 'qwen2.5:0.5b' },
  { provider: 'ollama', name: 'qwen2.5:7b' },
  { provider: 'ollama', name: 'llama3.2:3b' },
]

export function ModelSelector({ model, onSelect }: Props) {
  const activeLabel = `${model.provider}/${model.name}`

  return (
    <div className="group relative">
      <button
        type="button"
        className="flex items-center gap-2 rounded-full border border-outline-variant/80 bg-surface-container-high/80 px-3 py-1 font-mono text-[11px] uppercase tracking-[0.08em] text-on-surface-variant transition-colors hover:border-primary/50 hover:text-primary"
        aria-label={`Active model ${activeLabel}`}
      >
        <span className="h-1.5 w-1.5 rounded-full bg-primary shadow-[0_0_8px_rgba(0,219,233,0.9)]" />
        <span>MODEL: {model.name}</span>
        <Icon name="chevronDown" className="h-3.5 w-3.5" />
      </button>

      <div className="absolute right-0 top-full z-50 hidden pt-2 group-focus-within:block group-hover:block">
        <div className="flex min-w-[280px] flex-col gap-1 rounded-lg border border-outline-variant/70 bg-surface-container-high/95 p-2 shadow-[0_20px_40px_rgba(0,0,0,0.5)] backdrop-blur-[30px]">
          {SUGGESTED_MODELS.map((m) => (
            <button
              key={`${m.provider}/${m.name}`}
              type="button"
              className={`flex w-full items-center justify-between rounded-md border-l-2 px-3 py-2 text-left font-mono text-sm transition-colors ${
                m.provider === model.provider && m.name === model.name
                  ? 'border-primary bg-primary/10 text-primary shadow-[inset_16px_0_32px_-16px_rgba(0,240,255,0.35)]'
                  : 'border-transparent text-on-surface-variant hover:bg-surface-bright/40 hover:text-on-surface'
              }`}
              onClick={() => onSelect?.(m)}
            >
              <span className="flex items-center gap-3">
                <span className="h-1.5 w-1.5 rounded-full bg-primary/80 shadow-[0_0_8px_rgba(0,219,233,0.65)]" />
                {m.name}
              </span>
              <Icon
                name={m.provider === 'ollama' ? 'cpu' : 'cloud'}
                className="h-4 w-4 opacity-60"
              />
            </button>
          ))}
        </div>
      </div>
    </div>
  )
}
