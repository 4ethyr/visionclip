// application/index.ts

export { initializeSession, createLocalSession, applyEvents } from './SessionManager'
export type { SessionState } from './SessionManager'

export { startEventStream } from './EventStreamer'
export type { StreamCallback, ErrorCallback } from './EventStreamer'

export {
  sendAsk,
  sendVoiceTurn,
  cancelRun,
  cancelSpeech,
  selectModel,
  openUi,
  captureVoice,
  captureAndExplain,
  dismissConfirmation,
} from './CommandSender'

export {
  DEFAULT_FLOATING_APPEARANCE,
  loadSettings,
  normalizeFloatingAppearance,
  saveSettings,
} from './SettingsStore'
export type { FloatingAppearanceSettings, UserSettings } from './SettingsStore'
