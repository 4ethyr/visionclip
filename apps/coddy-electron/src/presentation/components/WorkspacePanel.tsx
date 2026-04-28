// Workspace panel: shows context items (files, screen captures, etc.)

import type { ContextItem } from '@/domain'
import { Icon } from './Icon'

interface Props {
  items: ContextItem[]
}

export function WorkspacePanel({ items }: Props) {
  return (
    <div className="flex flex-col h-full">
      <div className="px-5 py-3 border-b border-primary/10">
        <h2 className="text-sm font-semibold text-on-surface tracking-wide">
          Workspace Context
        </h2>
        <p className="text-xs text-on-surface-variant mt-0.5">
          Files, captures, and documents for the current session
        </p>
      </div>

      <div className="flex-1 overflow-y-auto p-3">
        {items.length === 0 ? (
          <div className="text-xs text-on-surface-variant/50 text-center py-8">
            No context items yet
          </div>
        ) : (
          <div className="flex flex-col gap-1">
            {items.map((item) => (
              <div
                key={item.id}
                className={`flex items-center gap-2 px-3 py-2 rounded-lg text-sm ${
                  item.sensitive
                    ? 'bg-yellow-500/5 border border-yellow-500/10'
                    : 'bg-surface-container-low'
                }`}
              >
                <Icon
                  name={item.sensitive ? 'lock' : 'file'}
                  className={`h-4 w-4 flex-shrink-0 ${
                    item.sensitive ? 'text-yellow-300' : 'text-primary/80'
                  }`}
                />
                <span className="text-on-surface truncate">{item.label}</span>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
