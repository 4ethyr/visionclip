import type { FloatingAppearanceSettings } from '@/application'
import {
  DEFAULT_FLOATING_APPEARANCE,
  normalizeFloatingAppearance,
} from '@/application'
import { Icon } from './Icon'

interface Props {
  value: FloatingAppearanceSettings
  onChange: (value: FloatingAppearanceSettings) => void
  onClose: () => void
}

export function FloatingSettingsModal({ value, onChange, onClose }: Props) {
  const update = (patch: Partial<FloatingAppearanceSettings>) => {
    onChange(normalizeFloatingAppearance({ ...value, ...patch }))
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/20 px-4 backdrop-blur-sm"
      role="dialog"
      aria-modal="true"
      aria-labelledby="floating-settings-title"
    >
      <section className="w-full max-w-xl overflow-hidden rounded-2xl border border-primary/25 bg-slate-950/80 text-on-surface backdrop-blur-2xl">
        <header className="flex items-center justify-between border-b border-primary/15 px-6 py-4">
          <div className="flex items-center gap-3">
            <span className="flex h-9 w-9 items-center justify-center rounded-lg border border-primary/30 bg-primary/10 text-primary">
              <Icon name="settings" className="h-4 w-4" />
            </span>
            <div>
              <h2
                id="floating-settings-title"
                className="font-display text-sm font-semibold uppercase tracking-[0.2em] text-primary"
              >
                Terminal settings
              </h2>
              <p className="mt-1 font-mono text-xs text-on-surface-variant">
                Appearance controls are applied instantly.
              </p>
            </div>
          </div>

          <button
            type="button"
            onClick={onClose}
            className="rounded-full p-2 text-on-surface-variant transition-colors hover:text-primary"
            aria-label="Close settings"
          >
            <Icon name="close" className="h-4 w-4" />
          </button>
        </header>

        <div className="space-y-5 px-6 py-5">
          <RangeField
            label="Blur"
            value={value.blurPx}
            min={0}
            max={48}
            step={1}
            suffix="px"
            onChange={(blurPx) => update({ blurPx })}
          />
          <RangeField
            label="Transparency"
            value={value.transparency}
            min={0.32}
            max={0.92}
            step={0.01}
            formatter={(current) => `${Math.round(current * 100)}%`}
            onChange={(transparency) => update({ transparency })}
          />
          <RangeField
            label="Glass effect"
            value={value.glassIntensity}
            min={0}
            max={0.32}
            step={0.01}
            formatter={(current) => `${Math.round(current * 100)}%`}
            onChange={(glassIntensity) => update({ glassIntensity })}
          />

          <div className="grid gap-4 sm:grid-cols-2">
            <ColorField
              label="Text color"
              value={value.textColor}
              onChange={(textColor) => update({ textColor })}
            />
            <ColorField
              label="Accent color"
              value={value.accentColor}
              onChange={(accentColor) => update({ accentColor })}
            />
          </div>
        </div>

        <footer className="flex items-center justify-between border-t border-primary/15 px-6 py-4">
          <button
            type="button"
            onClick={() => onChange({ ...DEFAULT_FLOATING_APPEARANCE })}
            className="rounded-lg border border-outline-variant/70 px-4 py-2 font-mono text-xs uppercase tracking-[0.16em] text-on-surface-variant transition-colors hover:border-primary/40 hover:text-primary"
          >
            Reset
          </button>
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg border border-primary/40 bg-primary/10 px-4 py-2 font-mono text-xs uppercase tracking-[0.16em] text-primary transition-colors hover:bg-primary/15"
          >
            Done
          </button>
        </footer>
      </section>
    </div>
  )
}

interface RangeFieldProps {
  label: string
  value: number
  min: number
  max: number
  step: number
  suffix?: string
  formatter?: (value: number) => string
  onChange: (value: number) => void
}

function RangeField({
  label,
  value,
  min,
  max,
  step,
  suffix = '',
  formatter,
  onChange,
}: RangeFieldProps) {
  const displayValue = formatter ? formatter(value) : `${value}${suffix}`

  return (
    <label className="block">
      <span className="mb-2 flex items-center justify-between font-mono text-xs uppercase tracking-[0.16em] text-on-surface-variant">
        <span>{label}</span>
        <span className="text-primary">{displayValue}</span>
      </span>
      <input
        aria-label={label}
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(event) => onChange(Number(event.currentTarget.value))}
        className="w-full accent-primary"
      />
    </label>
  )
}

interface ColorFieldProps {
  label: string
  value: string
  onChange: (value: string) => void
}

function ColorField({ label, value, onChange }: ColorFieldProps) {
  return (
    <label className="block rounded-xl border border-outline-variant/50 bg-surface-container/30 p-3">
      <span className="mb-3 block font-mono text-xs uppercase tracking-[0.16em] text-on-surface-variant">
        {label}
      </span>
      <div className="flex items-center gap-3">
        <input
          aria-label={label}
          type="color"
          value={value}
          onChange={(event) => onChange(event.currentTarget.value)}
          className="h-9 w-12 cursor-pointer rounded border border-outline-variant/70 bg-transparent"
        />
        <span className="font-mono text-sm text-on-surface">{value}</span>
      </div>
    </label>
  )
}
