import { describe, it, expect } from 'vitest'
import type { ReplEvent, ReplEventEnvelope, ReplIntent, ToolStatus, ShortcutSource, ExtractionSource } from '@/domain/types/events'
import type { SessionStatus, AssessmentPolicy } from '@/domain/types/session'
import type { RequestedHelp, AssistanceFallback } from '@/domain/types/policy'

describe('Domain type contracts', () => {
  describe('ReplEvent discriminated union', () => {
    it('all event variants are valid ReplEvent types', () => {
      const events: { [K in keyof ReplEvent]: ReplEvent } = {
        SessionStarted: { SessionStarted: { session_id: 'uuid' } },
        RunStarted: { RunStarted: { run_id: 'uuid' } },
        ShortcutTriggered: { ShortcutTriggered: { binding: 'Ctrl+Space', source: 'Cli' as ShortcutSource } },
        OverlayShown: { OverlayShown: { mode: 'FloatingTerminal' } },
        VoiceListeningStarted: { VoiceListeningStarted: {} },
        VoiceTranscriptPartial: { VoiceTranscriptPartial: { text: 'hello' } },
        VoiceTranscriptFinal: { VoiceTranscriptFinal: { text: 'hello world' } },
        ScreenCaptured: { ScreenCaptured: { source: 'ScreenshotOcr' as ExtractionSource, bytes: 1024 } },
        OcrCompleted: { OcrCompleted: { chars: 500 } },
        IntentDetected: { IntentDetected: { intent: 'OpenApplication' as ReplIntent, confidence: 0.95 } },
        PolicyEvaluated: { PolicyEvaluated: { policy: 'Practice', allowed: true } },
        ConfirmationDismissed: { ConfirmationDismissed: {} },
        ModelSelected: {
          ModelSelected: {
            model: { provider: 'ollama', name: 'qwen2.5:0.5b' },
            role: 'Chat',
          },
        },
        SearchStarted: { SearchStarted: { query: 'Rust docs', provider: 'google' } },
        SearchContextExtracted: { SearchContextExtracted: { provider: 'google', organic_results: 5, ai_overview_present: false } },
        TokenDelta: { TokenDelta: { run_id: 'run-1', text: 'Hello' } },
        MessageAppended: { MessageAppended: { message: { id: 'm1', role: 'user', text: 'hi' } } },
        ToolStarted: { ToolStarted: { name: 'search_web' } },
        ToolCompleted: { ToolCompleted: { name: 'search_web', status: 'Succeeded' as ToolStatus } },
        TtsQueued: { TtsQueued: {} },
        TtsStarted: { TtsStarted: {} },
        TtsCompleted: { TtsCompleted: {} },
        RunCompleted: { RunCompleted: { run_id: 'uuid' } },
        Error: { Error: { code: 'E001', message: 'Something went wrong' } },
      }

      expect(Object.keys(events)).toHaveLength(24)

      // Verify each event is correctly typed
      for (const [key, event] of Object.entries(events)) {
        expect(Object.keys(event as object)).toHaveLength(1)
        expect(Object.keys(event as object)[0]).toBe(key)
      }
    })
  })

  describe('ReplEventEnvelope', () => {
    it('has the correct shape', () => {
      const envelope: ReplEventEnvelope = {
        sequence: 1,
        session_id: 'uuid-session',
        run_id: 'uuid-run',
        captured_at_unix_ms: 1775000000000,
        event: { SessionStarted: { session_id: 'uuid' } },
      }

      expect(envelope.sequence).toBe(1)
      expect(envelope.session_id).toBe('uuid-session')
      expect(envelope.run_id).toBe('uuid-run')
      expect(envelope.captured_at_unix_ms).toBeGreaterThan(0)
    })

    it('allows null run_id', () => {
      const envelope: ReplEventEnvelope = {
        sequence: 1,
        session_id: 'uuid',
        run_id: null,
        captured_at_unix_ms: 0,
        event: { SessionStarted: { session_id: 'uuid' } },
      }

      expect(envelope.run_id).toBeNull()
    })
  })

  describe('SessionStatus', () => {
    it('has all 10 values', () => {
      const statuses: SessionStatus[] = [
        'Idle', 'Listening', 'Transcribing', 'CapturingScreen',
        'BuildingContext', 'Thinking', 'Streaming', 'Speaking',
        'AwaitingConfirmation', 'Error',
      ]
      expect(statuses).toHaveLength(10)
      expect(new Set(statuses).size).toBe(10) // all unique
    })
  })

  describe('AssessmentPolicy', () => {
    it('has all 5 values', () => {
      const policies: AssessmentPolicy[] = [
        'Practice', 'PermittedAi', 'SyntaxOnly',
        'RestrictedAssessment', 'UnknownAssessment',
      ]
      expect(policies).toHaveLength(5)
      expect(new Set(policies).size).toBe(5)
    })
  })

  describe('RequestedHelp', () => {
    it('has all 5 values', () => {
      const helpTypes: RequestedHelp[] = [
        'ExplainConcept', 'SolveMultipleChoice', 'GenerateCompleteCode',
        'DebugCode', 'GenerateTests',
      ]
      expect(helpTypes).toHaveLength(5)
    })
  })

  describe('AssistanceFallback', () => {
    it('has all 4 values', () => {
      const fallbacks: AssistanceFallback[] = [
        'None', 'ConceptualGuidance', 'SyntaxOnlyGuidance', 'AskForPolicyConfirmation',
      ]
      expect(fallbacks).toHaveLength(4)
    })
  })
})
