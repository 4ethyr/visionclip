// domain/types/policy.ts
// Mirrors: crates/coddy-core/src/assessment.rs + policy.rs

export type AssessmentPolicy =
  | 'Practice'
  | 'PermittedAi'
  | 'SyntaxOnly'
  | 'RestrictedAssessment'
  | 'UnknownAssessment'

export type RequestedHelp =
  | 'ExplainConcept'
  | 'SolveMultipleChoice'
  | 'GenerateCompleteCode'
  | 'DebugCode'
  | 'GenerateTests'

export type ScreenAssistMode =
  | 'ExplainVisibleScreen'
  | 'ExplainCode'
  | 'DebugError'
  | 'MultipleChoice'
  | 'SummarizeDocument'

export type AssistanceFallback =
  | 'None'
  | 'ConceptualGuidance'
  | 'SyntaxOnlyGuidance'
  | 'AskForPolicyConfirmation'

export interface AssistanceDecision {
  allowed: boolean
  requiresConfirmation: boolean
  fallback: AssistanceFallback
  reason: string
}

export function allow(reason: string): AssistanceDecision {
  return { allowed: true, requiresConfirmation: false, fallback: 'None', reason }
}

export function block(fallback: AssistanceFallback, reason: string): AssistanceDecision {
  return { allowed: false, requiresConfirmation: false, fallback, reason }
}

export function confirm(fallback: AssistanceFallback, reason: string): AssistanceDecision {
  return { allowed: false, requiresConfirmation: true, fallback, reason }
}
