// domain/reducers/policyEvaluator.ts
// Pure function: Assessment × RequestedHelp → AssistanceDecision
// Mirrors: crates/coddy-core/src/policy.rs — evaluate_assistance()

import type { AssessmentPolicy, RequestedHelp, AssistanceDecision } from '@/domain/types/policy'
import { allow, block, confirm } from '@/domain/types/policy'

export function evaluateAssistance(
  policy: AssessmentPolicy,
  requestedHelp: RequestedHelp,
): AssistanceDecision {
  switch (policy) {
    case 'Practice':
      return allow(
        `Practice mode — ${requestedHelp} is always allowed.`,
      )

    case 'PermittedAi':
      return allow(
        `AI is explicitly permitted — ${requestedHelp} is allowed.`,
      )

    case 'SyntaxOnly':
      switch (requestedHelp) {
        case 'ExplainConcept':
          return allow('Conceptual explanations are allowed under SyntaxOnly.')
        case 'DebugCode':
          return allow('Debugging guidance is allowed under SyntaxOnly.')
        case 'GenerateTests':
          return block(
            'SyntaxOnlyGuidance',
            'Generating test cases exceeds syntax-level help. I can guide you on test syntax instead.',
          )
        case 'SolveMultipleChoice':
          return block(
            'SyntaxOnlyGuidance',
            'Answering multiple choice questions requires conceptual reasoning beyond syntax help.',
          )
        case 'GenerateCompleteCode':
          return block(
            'SyntaxOnlyGuidance',
            'Generating complete code is blocked under SyntaxOnly policy.',
          )
        default:
          return unreachableRequestedHelp(requestedHelp)
      }

    case 'RestrictedAssessment':
      switch (requestedHelp) {
        case 'ExplainConcept':
          return allow('Conceptual explanations are allowed under restricted assessment.')
        case 'DebugCode':
          return allow('Debugging help is allowed under restricted assessment.')
        case 'GenerateTests':
          return block(
            'ConceptualGuidance',
            'Test generation is restricted. I can explain testing concepts instead.',
          )
        case 'SolveMultipleChoice':
          return block(
            'ConceptualGuidance',
            'Direct answers to multiple choice are blocked during restricted assessment.',
          )
        case 'GenerateCompleteCode':
          return block(
            'ConceptualGuidance',
            'Writing complete solutions is blocked during restricted assessment.',
          )
        default:
          return unreachableRequestedHelp(requestedHelp)
      }

    case 'UnknownAssessment':
      return confirm(
        'AskForPolicyConfirmation',
        `This assessment environment is not configured. Please confirm that AI assistance for ${requestedHelp} is permitted.`,
      )
  }
}

function unreachableRequestedHelp(value: never): never {
  throw new Error(`Unhandled requested help: ${value}`)
}
