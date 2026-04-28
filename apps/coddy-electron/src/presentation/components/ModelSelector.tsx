// Model selector: shows current model and allows switching.
// Stub for now — model list comes from the daemon later.

import type { ModelRef } from '@/domain'

interface Props {
  model: ModelRef
  onSelect?: (model: ModelRef) => void
}

const SUGGESTED_MODELS: ModelRef[] = [
  { provider: 'ollama', name: 'gemma4:e2b' },
  { provider: 'ollama', name: 'llama3.2:3b' },
  { provider: 'ollama', name: 'qwen2.5:7b' },
  { provider: 'openai', name: 'gpt-4o-mini' },
]

export function ModelSelector({ model, onSelect }: Props) {
  return (
    <div className="relative group">
      <span className="text-xs text-on-surface-variant bg-surface-container-high px-2.5 py-0.5 rounded-full border border-outline-variant cursor-pointer hover:border-primary/30 transition-colors">
        {model.provider}/{model.name}
      </span>

      {/* Dropdown on hover */}
      <div className="absolute right-0 top-full mt-1 hidden group-hover:block z-50">
        <div className="bg-surface-container-high border border-outline-variant rounded-lg py-1 shadow-xl min-w-[160px]">
          {SUGGESTED_MODELS.map((m) => (
            <button
              key={`${m.provider}/${m.name}`}
              type="button"
              className={`w-full text-left px-3 py-1.5 text-xs hover:bg-surface-container transition-colors ${
                m.provider === model.provider && m.name === model.name
                  ? 'text-primary'
                  : 'text-on-surface-variant'
              }`}
              onClick={() => onSelect?.(m)}
            >
              {m.provider}/{m.name}
            </button>
          ))}
        </div>
      </div>
    </div>
  )
}