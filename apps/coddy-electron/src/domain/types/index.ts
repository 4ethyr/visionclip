// domain/types/index.ts — barrel exports

export type {
  ShortcutSource,
  ReplIntent,
  ToolStatus,
  ReplMode,
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
  ModelRef,
  VoiceState,
  ContextItem,
  ScreenUnderstandingContext,
  ReplSession,
} from './session'

export { createInitialSession } from './session'

export type {
  AssessmentPolicy,
  RequestedHelp,
  AssistanceFallback,
  AssistanceDecision,
} from './policy'

export { allow, block, confirm } from './policy'