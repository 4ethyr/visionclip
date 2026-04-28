import type { JSX, SVGProps } from 'react'

export type IconName =
  | 'alert'
  | 'bot'
  | 'chevronDown'
  | 'close'
  | 'cloud'
  | 'copy'
  | 'cpu'
  | 'desktop'
  | 'file'
  | 'lock'
  | 'maximize'
  | 'mic'
  | 'minimize'
  | 'screen'
  | 'send'
  | 'sensors'
  | 'settings'
  | 'terminal'
  | 'user'

interface Props extends SVGProps<SVGSVGElement> {
  name: IconName
}

const PATHS: Record<IconName, JSX.Element> = {
  alert: (
    <>
      <path d="M12 9v4" />
      <path d="M12 17h.01" />
      <path d="m10.3 3.9-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.7-3.1l-8-14a2 2 0 0 0-3.4 0Z" />
    </>
  ),
  bot: (
    <>
      <path d="M12 8V4" />
      <rect x="5" y="8" width="14" height="10" rx="2" />
      <path d="M8 18v2" />
      <path d="M16 18v2" />
      <path d="M9 12h.01" />
      <path d="M15 12h.01" />
      <path d="M10 15h4" />
    </>
  ),
  chevronDown: <path d="m6 9 6 6 6-6" />,
  close: (
    <>
      <path d="M18 6 6 18" />
      <path d="m6 6 12 12" />
    </>
  ),
  cloud: (
    <>
      <path d="M17.5 19H8a5 5 0 1 1 1.1-9.9A6 6 0 0 1 20 12.4 3.5 3.5 0 0 1 17.5 19Z" />
    </>
  ),
  copy: (
    <>
      <rect x="9" y="9" width="11" height="11" rx="2" />
      <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
    </>
  ),
  cpu: (
    <>
      <rect x="7" y="7" width="10" height="10" rx="1" />
      <path d="M9 1v3" />
      <path d="M15 1v3" />
      <path d="M9 20v3" />
      <path d="M15 20v3" />
      <path d="M20 9h3" />
      <path d="M20 14h3" />
      <path d="M1 9h3" />
      <path d="M1 14h3" />
      <path d="M10 10h4v4h-4z" />
    </>
  ),
  desktop: (
    <>
      <rect x="3" y="4" width="18" height="12" rx="2" />
      <path d="M8 20h8" />
      <path d="M12 16v4" />
    </>
  ),
  file: (
    <>
      <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z" />
      <path d="M14 2v6h6" />
      <path d="M8 13h8" />
      <path d="M8 17h5" />
    </>
  ),
  lock: (
    <>
      <rect x="4" y="11" width="16" height="10" rx="2" />
      <path d="M8 11V7a4 4 0 0 1 8 0v4" />
    </>
  ),
  maximize: (
    <>
      <path d="M8 3H5a2 2 0 0 0-2 2v3" />
      <path d="M16 3h3a2 2 0 0 1 2 2v3" />
      <path d="M8 21H5a2 2 0 0 1-2-2v-3" />
      <path d="M16 21h3a2 2 0 0 0 2-2v-3" />
    </>
  ),
  mic: (
    <>
      <path d="M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3Z" />
      <path d="M19 10v2a7 7 0 0 1-14 0v-2" />
      <path d="M12 19v4" />
      <path d="M8 23h8" />
    </>
  ),
  minimize: <path d="M5 12h14" />,
  screen: (
    <>
      <path d="M21 14V6a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v8" />
      <path d="M3 18h18" />
      <path d="M8 22h8" />
      <path d="M12 18v4" />
    </>
  ),
  send: (
    <>
      <path d="m5 12 7-7 7 7" />
      <path d="M12 19V5" />
    </>
  ),
  sensors: (
    <>
      <path d="M12 8v8" />
      <path d="M8.5 9.5a5 5 0 0 0 0 5" />
      <path d="M15.5 9.5a5 5 0 0 1 0 5" />
      <path d="M5.6 6.6a9 9 0 0 0 0 10.8" />
      <path d="M18.4 6.6a9 9 0 0 1 0 10.8" />
    </>
  ),
  settings: (
    <>
      <path d="M12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Z" />
      <path d="M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.9-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1-1.5 1.7 1.7 0 0 0-1.9.3l-.1.1A2 2 0 1 1 4.2 17l.1-.1a1.7 1.7 0 0 0 .3-1.9 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1 1.7 1.7 0 0 0-.3-1.9l-.1-.1A2 2 0 1 1 7 4.2l.1.1a1.7 1.7 0 0 0 1.9.3 1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.9-.3l.1-.1A2 2 0 1 1 19.8 7l-.1.1a1.7 1.7 0 0 0-.3 1.9 1.7 1.7 0 0 0 1.5 1h.1a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1Z" />
    </>
  ),
  terminal: (
    <>
      <rect x="3" y="5" width="18" height="14" rx="2" />
      <path d="m7 10 3 2-3 2" />
      <path d="M13 15h4" />
    </>
  ),
  user: (
    <>
      <path d="M20 21a8 8 0 0 0-16 0" />
      <circle cx="12" cy="7" r="4" />
    </>
  ),
}

export function Icon({ name, className, ...props }: Props) {
  return (
    <svg
      aria-hidden="true"
      className={className}
      fill="none"
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth="2"
      viewBox="0 0 24 24"
      {...props}
    >
      {PATHS[name]}
    </svg>
  )
}
