// domain/reducers/sessionReducer.ts
// Pure function: ReplSession × ReplEvent → ReplSession
// Mirrors: crates/coddy-core/src/session.rs — ReplSession::apply_event()

import type { ReplSession } from '@/domain/types/session'
import type { ReplEvent } from '@/domain/types/events'

export function sessionReducer(session: ReplSession, event: ReplEvent): ReplSession {
  const tag = Object.keys(event)[0] as keyof ReplEvent

  switch (tag) {
    case 'SessionStarted': {
      const { session_id } = (event as { SessionStarted: { session_id: string } }).SessionStarted
      return { ...session, id: session_id, status: 'Idle' }
    }

    case 'RunStarted': {
      const { run_id } = (event as { RunStarted: { run_id: string } }).RunStarted
      return { ...session, active_run: run_id, status: 'Thinking', streaming_text: '' }
    }

    case 'VoiceListeningStarted':
      return { ...session, status: 'Listening' }

    case 'VoiceTranscriptPartial':
      return { ...session, status: 'Transcribing' }

    case 'VoiceTranscriptFinal':
      return { ...session, status: 'Thinking' }

    case 'IntentDetected':
      return { ...session, status: 'Thinking' }

    case 'SearchStarted':
      return { ...session, status: 'Thinking' }

    case 'SearchContextExtracted':
      return { ...session, status: 'BuildingContext' }

    case 'TokenDelta': {
      const { text } = (event as { TokenDelta: { run_id: string; text: string } }).TokenDelta
      return { ...session, status: 'Streaming', streaming_text: session.streaming_text + text }
    }

    case 'MessageAppended': {
      const msg = (event as { MessageAppended: { message: ReplSession['messages'][number] } })
        .MessageAppended.message
      return { ...session, messages: [...session.messages, msg], streaming_text: '' }
    }

    case 'ToolStarted':
    case 'ToolCompleted':
      // Tool events do not change status — the backend manages tool lifecycle
      return session

    case 'TtsStarted':
      return {
        ...session,
        voice: { ...session.voice, speaking: true },
        status: 'Speaking',
      }

    case 'TtsCompleted': {
      const newVoice = { ...session.voice, speaking: false }
      const newStatus = session.active_run ? 'Streaming' : 'Idle'
      return { ...session, voice: newVoice, status: newStatus }
    }

    case 'RunCompleted': {
      const newStatus = session.voice.speaking ? 'Speaking' : 'Idle'
      return { ...session, active_run: null, status: newStatus, streaming_text: '' }
    }

    case 'Error':
      return { ...session, status: 'Error' }

    case 'OverlayShown': {
      const { mode } = (event as { OverlayShown: { mode: ReplSession['mode'] } }).OverlayShown
      return { ...session, mode }
    }

    case 'PolicyEvaluated': {
      const { policy, allowed } = (event as { PolicyEvaluated: { policy: string; allowed: boolean } })
        .PolicyEvaluated
      return {
        ...session,
        policy: policy as ReplSession['policy'],
        status: !allowed && policy === 'UnknownAssessment'
          ? 'AwaitingConfirmation'
          : session.status,
      }
    }

    case 'ConfirmationDismissed':
      return session.status === 'AwaitingConfirmation'
        ? { ...session, status: 'Idle' }
        : session

    case 'ModelSelected': {
      const { model, role } = (event as { ModelSelected: {
        model: ReplSession['selected_model']
        role: string
      } }).ModelSelected

      if (role !== 'Chat') return session

      return { ...session, selected_model: model }
    }

    // Events that the frontend observes but does not mutate state for:
    case 'ShortcutTriggered':
    case 'ScreenCaptured':
    case 'OcrCompleted':
    case 'TtsQueued':
      return session

    default:
      return session
  }
}
