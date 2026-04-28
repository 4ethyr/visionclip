// domain/types/index.ts — barrel exports

export type {
  ShortcutSource,
  ReplIntent,
  ToolStatus,
  ReplMode,
  ModelRef,
  ModelRole,
  ExtractionSource,
  ReplMessage,
  ReplEvent,
  ReplEventEnvelope,
  ReplSessionSnapshot,
  ReplSessionSnapshotSession,
} from './events'

export type {
  SessionStatus,
  AssessmentPolicy as SessionAssessmentPolicy,
  VoiceState,
  ContextItem,
  ScreenUnderstandingContext,
  ReplSession,
} from './session'

export { createInitialSession } from './session'

export type {
  AssessmentPolicy,
  RequestedHelp,
  ScreenAssistMode,
  AssistanceFallback,
  AssistanceDecision,
} from './policy'

export { allow, block, confirm } from './policy'
