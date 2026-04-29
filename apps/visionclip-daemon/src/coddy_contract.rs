#![allow(dead_code)]

// Mirrors the Coddy wire protocol while VisionClip still serves the temporary
// Coddy bridge. Keep this module in sync with Coddy's published IPC contract.

use anyhow::{bail, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::sync::broadcast;
use uuid::Uuid;

const CODDY_PROTOCOL_VERSION: u16 = 1;
const CODDY_PROTOCOL_MAGIC: [u8; 4] = *b"CDDY";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(super) struct CoddyWireRequest {
    pub magic: [u8; 4],
    pub protocol_version: u16,
    pub request: CoddyRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(super) struct CoddyWireResult {
    pub magic: [u8; 4],
    pub protocol_version: u16,
    pub result: CoddyResult,
}

impl CoddyWireRequest {
    pub fn new(request: CoddyRequest) -> Self {
        Self {
            magic: CODDY_PROTOCOL_MAGIC,
            protocol_version: CODDY_PROTOCOL_VERSION,
            request,
        }
    }

    fn ensure_compatible(&self) -> Result<()> {
        ensure_wire_compatible(self.magic, self.protocol_version)
    }
}

impl CoddyWireResult {
    pub fn new(result: CoddyResult) -> Self {
        Self {
            magic: CODDY_PROTOCOL_MAGIC,
            protocol_version: CODDY_PROTOCOL_VERSION,
            result,
        }
    }
}

fn ensure_wire_compatible(magic: [u8; 4], protocol_version: u16) -> Result<()> {
    if magic != CODDY_PROTOCOL_MAGIC {
        bail!("invalid Coddy protocol magic: {magic:?}");
    }
    if protocol_version != CODDY_PROTOCOL_VERSION {
        bail!(
            "incompatible Coddy protocol version: expected {}, got {}",
            CODDY_PROTOCOL_VERSION,
            protocol_version
        );
    }
    Ok(())
}

pub(super) async fn write_frame<W, T>(writer: &mut W, value: &T) -> Result<()>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let payload = encode_payload(value)?;
    let length = payload.len() as u32;
    writer.write_u32(length).await?;
    writer.write_all(&payload).await?;
    writer.flush().await?;
    Ok(())
}

pub(super) fn encode_payload<T>(value: &T) -> Result<Vec<u8>>
where
    T: Serialize,
{
    Ok(bincode::serde::encode_to_vec(
        value,
        bincode::config::standard(),
    )?)
}

fn decode_payload<T>(payload: &[u8]) -> Result<T>
where
    T: DeserializeOwned,
{
    let (value, decoded): (T, usize) =
        bincode::serde::decode_from_slice(payload, bincode::config::standard())?;
    ensure_fully_decoded(decoded, payload.len())?;
    Ok(value)
}

pub(super) fn decode_wire_request_payload(payload: &[u8]) -> Result<Option<CoddyRequest>> {
    if let Ok((request, decoded)) = bincode::serde::decode_from_slice::<CoddyWireRequest, _>(
        payload,
        bincode::config::standard(),
    ) {
        if request.magic == CODDY_PROTOCOL_MAGIC {
            ensure_fully_decoded(decoded, payload.len())?;
            request.ensure_compatible()?;
            return Ok(Some(request.request));
        }
    }

    Ok(None)
}

fn ensure_fully_decoded(decoded: usize, total: usize) -> Result<()> {
    if decoded == total {
        Ok(())
    } else {
        bail!("trailing bytes after Coddy payload: decoded {decoded} of {total} bytes");
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(super) struct ReplCommandJob {
    pub request_id: Uuid,
    pub command: ReplCommand,
    pub speak: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct ReplSessionSnapshotJob {
    pub request_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct ReplEventsJob {
    pub request_id: Uuid,
    pub after_sequence: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct ReplEventStreamJob {
    pub request_id: Uuid,
    pub after_sequence: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct ReplToolsJob {
    pub request_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(super) enum CoddyRequest {
    Command(ReplCommandJob),
    SessionSnapshot(ReplSessionSnapshotJob),
    Events(ReplEventsJob),
    EventStream(ReplEventStreamJob),
    Tools(ReplToolsJob),
}

impl CoddyRequest {
    pub fn request_id(&self) -> Uuid {
        match self {
            Self::Command(job) => job.request_id,
            Self::SessionSnapshot(job) => job.request_id,
            Self::Events(job) => job.request_id,
            Self::EventStream(job) => job.request_id,
            Self::Tools(job) => job.request_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(super) enum CoddyResult {
    Text {
        request_id: Uuid,
        text: String,
        spoken: bool,
    },
    BrowserQuery {
        request_id: Uuid,
        query: String,
        summary: Option<String>,
        spoken: bool,
    },
    ActionStatus {
        request_id: Uuid,
        message: String,
        spoken: bool,
    },
    Error {
        request_id: Uuid,
        code: String,
        message: String,
    },
    ReplSessionSnapshot {
        request_id: Uuid,
        snapshot: Box<ReplSessionSnapshot>,
    },
    ReplEvents {
        request_id: Uuid,
        events: Vec<ReplEventEnvelope>,
        last_sequence: u64,
    },
    ReplTools {
        request_id: Uuid,
        tools: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub(super) enum AssessmentPolicy {
    Practice,
    PermittedAi,
    SyntaxOnly,
    RestrictedAssessment,
    UnknownAssessment,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub(super) enum RequestedHelp {
    ExplainConcept,
    SolveMultipleChoice,
    GenerateCompleteCode,
    DebugCode,
    GenerateTests,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AssistanceDecision {
    pub allowed: bool,
    pub requires_confirmation: bool,
    pub reason: String,
}

impl AssistanceDecision {
    fn allow(reason: impl Into<String>) -> Self {
        Self {
            allowed: true,
            requires_confirmation: false,
            reason: reason.into(),
        }
    }

    fn block(reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            requires_confirmation: false,
            reason: reason.into(),
        }
    }

    fn confirm(reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            requires_confirmation: true,
            reason: reason.into(),
        }
    }
}

pub(super) fn evaluate_assistance(
    policy: AssessmentPolicy,
    requested_help: RequestedHelp,
) -> AssistanceDecision {
    match policy {
        AssessmentPolicy::Practice | AssessmentPolicy::PermittedAi => {
            AssistanceDecision::allow("assistance allowed by current assessment policy")
        }
        AssessmentPolicy::SyntaxOnly => match requested_help {
            RequestedHelp::ExplainConcept | RequestedHelp::DebugCode => {
                AssistanceDecision::allow("syntax and conceptual help are allowed")
            }
            RequestedHelp::GenerateTests
            | RequestedHelp::SolveMultipleChoice
            | RequestedHelp::GenerateCompleteCode => {
                AssistanceDecision::block("current policy only allows syntax-level guidance")
            }
        },
        AssessmentPolicy::RestrictedAssessment => match requested_help {
            RequestedHelp::ExplainConcept | RequestedHelp::DebugCode => {
                AssistanceDecision::allow("conceptual help is allowed without final answers")
            }
            RequestedHelp::GenerateTests
            | RequestedHelp::SolveMultipleChoice
            | RequestedHelp::GenerateCompleteCode => AssistanceDecision::block(
                "restricted assessments must not receive final answers or complete code",
            ),
        },
        AssessmentPolicy::UnknownAssessment => {
            AssistanceDecision::confirm("assessment policy is unknown and requires confirmation")
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub(super) struct ModelRef {
    pub provider: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub(super) enum ModelRole {
    Chat,
    Ocr,
    Asr,
    Tts,
    Embedding,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub(super) enum ContextPolicy {
    NoScreen,
    VisibleScreen,
    WorkspaceOnly,
    ScreenAndWorkspace,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub(super) enum ScreenAssistMode {
    ExplainVisibleScreen,
    ExplainCode,
    DebugError,
    MultipleChoice,
    SummarizeDocument,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) enum ReplCommand {
    Ask {
        text: String,
        context_policy: ContextPolicy,
    },
    CaptureAndExplain {
        mode: ScreenAssistMode,
        policy: AssessmentPolicy,
    },
    VoiceTurn {
        transcript_override: Option<String>,
    },
    OpenUi {
        mode: ReplMode,
    },
    SelectModel {
        model: ModelRef,
        role: ModelRole,
    },
    DismissConfirmation,
    StopActiveRun,
    StopSpeaking,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub(super) enum ShortcutSource {
    GnomeMediaKeys,
    TauriGlobalShortcut,
    Cli,
    SystemdUserService,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub(super) enum ReplIntent {
    AskTechnicalQuestion,
    ExplainScreen,
    ExplainCode,
    DebugCode,
    SolvePracticeQuestion,
    MultipleChoiceAssist,
    GenerateTestCases,
    ExplainTerminalError,
    SearchDocs,
    OpenApplication,
    OpenWebsite,
    ConfigureModel,
    ManageContext,
    AgenticCodeChange,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub(super) enum ToolStatus {
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(super) enum ReplEvent {
    SessionStarted {
        session_id: Uuid,
    },
    RunStarted {
        run_id: Uuid,
    },
    ShortcutTriggered {
        binding: String,
        source: ShortcutSource,
    },
    OverlayShown {
        mode: ReplMode,
    },
    VoiceListeningStarted,
    VoiceTranscriptPartial {
        text: String,
    },
    VoiceTranscriptFinal {
        text: String,
    },
    ScreenCaptured {
        source: ExtractionSource,
        bytes: usize,
    },
    OcrCompleted {
        chars: usize,
        language_hint: Option<String>,
    },
    IntentDetected {
        intent: ReplIntent,
        confidence: f32,
    },
    PolicyEvaluated {
        policy: AssessmentPolicy,
        allowed: bool,
    },
    ConfirmationDismissed,
    ModelSelected {
        model: ModelRef,
        role: ModelRole,
    },
    SearchStarted {
        query: String,
        provider: String,
    },
    SearchContextExtracted {
        provider: String,
        organic_results: usize,
        ai_overview_present: bool,
    },
    TokenDelta {
        run_id: Uuid,
        text: String,
    },
    MessageAppended {
        message: ReplMessage,
    },
    ToolStarted {
        name: String,
    },
    ToolCompleted {
        name: String,
        status: ToolStatus,
    },
    TtsQueued,
    TtsStarted,
    TtsCompleted,
    RunCompleted {
        run_id: Uuid,
    },
    Error {
        code: String,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub(super) enum ReplMode {
    FloatingTerminal,
    DesktopApp,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub(super) enum SessionStatus {
    Idle,
    Listening,
    Transcribing,
    CapturingScreen,
    BuildingContext,
    Thinking,
    Streaming,
    Speaking,
    AwaitingConfirmation,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct VoiceState {
    pub enabled: bool,
    pub speaking: bool,
    pub muted: bool,
}

impl Default for VoiceState {
    fn default() -> Self {
        Self {
            enabled: true,
            speaking: false,
            muted: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct ContextItem {
    pub id: String,
    pub label: String,
    pub sensitive: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct ReplMessage {
    pub id: Uuid,
    pub role: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(super) struct ReplSession {
    pub id: Uuid,
    pub mode: ReplMode,
    pub status: SessionStatus,
    pub policy: AssessmentPolicy,
    pub selected_model: ModelRef,
    pub voice: VoiceState,
    pub screen_context: Option<ScreenUnderstandingContext>,
    pub workspace_context: Vec<ContextItem>,
    pub messages: Vec<ReplMessage>,
    pub active_run: Option<Uuid>,
}

impl ReplSession {
    pub fn new(mode: ReplMode, selected_model: ModelRef) -> Self {
        Self {
            id: Uuid::new_v4(),
            mode,
            status: SessionStatus::Idle,
            policy: AssessmentPolicy::UnknownAssessment,
            selected_model,
            voice: VoiceState::default(),
            screen_context: None,
            workspace_context: Vec::new(),
            messages: Vec::new(),
            active_run: None,
        }
    }

    pub fn apply_event(&mut self, event: &ReplEvent) {
        match event {
            ReplEvent::SessionStarted { session_id } => {
                self.id = *session_id;
                self.status = SessionStatus::Idle;
            }
            ReplEvent::RunStarted { run_id } => {
                self.active_run = Some(*run_id);
                self.status = SessionStatus::Thinking;
            }
            ReplEvent::ShortcutTriggered { .. } => {}
            ReplEvent::OverlayShown { mode } => {
                self.mode = *mode;
            }
            ReplEvent::VoiceListeningStarted => {
                self.status = SessionStatus::Listening;
            }
            ReplEvent::VoiceTranscriptPartial { .. } => {
                self.status = SessionStatus::Transcribing;
            }
            ReplEvent::VoiceTranscriptFinal { .. } => {
                self.status = SessionStatus::Thinking;
            }
            ReplEvent::ScreenCaptured { .. } => {
                self.status = SessionStatus::CapturingScreen;
            }
            ReplEvent::OcrCompleted { .. } => {
                self.status = SessionStatus::BuildingContext;
            }
            ReplEvent::IntentDetected { .. } => {
                self.status = SessionStatus::Thinking;
            }
            ReplEvent::PolicyEvaluated { policy, allowed } => {
                self.policy = *policy;
                if !allowed && *policy == AssessmentPolicy::UnknownAssessment {
                    self.status = SessionStatus::AwaitingConfirmation;
                }
            }
            ReplEvent::ConfirmationDismissed => {
                if self.status == SessionStatus::AwaitingConfirmation {
                    self.status = SessionStatus::Idle;
                }
            }
            ReplEvent::ModelSelected { model, role } => {
                if *role == ModelRole::Chat {
                    self.selected_model = model.clone();
                }
            }
            ReplEvent::SearchStarted { .. } => {
                self.status = SessionStatus::Thinking;
            }
            ReplEvent::SearchContextExtracted { .. } => {
                self.status = SessionStatus::BuildingContext;
            }
            ReplEvent::TokenDelta { run_id, .. } => {
                self.active_run.get_or_insert(*run_id);
                self.status = SessionStatus::Streaming;
            }
            ReplEvent::MessageAppended { message } => {
                self.messages.push(message.clone());
            }
            ReplEvent::ToolStarted { .. } => {
                self.status = SessionStatus::Thinking;
            }
            ReplEvent::ToolCompleted { .. } => {
                self.status = SessionStatus::Thinking;
            }
            ReplEvent::TtsQueued => {}
            ReplEvent::TtsStarted => {
                self.voice.speaking = true;
                self.status = SessionStatus::Speaking;
            }
            ReplEvent::TtsCompleted => {
                self.voice.speaking = false;
                self.status = if self.active_run.is_some() {
                    SessionStatus::Streaming
                } else {
                    SessionStatus::Idle
                };
            }
            ReplEvent::RunCompleted { run_id } => {
                if self.active_run == Some(*run_id) {
                    self.active_run = None;
                }
                self.status = if self.voice.speaking {
                    SessionStatus::Speaking
                } else {
                    SessionStatus::Idle
                };
            }
            ReplEvent::Error { .. } => {
                self.status = SessionStatus::Error;
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(super) struct ReplEventEnvelope {
    pub sequence: u64,
    pub session_id: Uuid,
    pub run_id: Option<Uuid>,
    pub captured_at_unix_ms: u64,
    pub event: ReplEvent,
}

impl ReplEventEnvelope {
    fn new(
        sequence: u64,
        session_id: Uuid,
        run_id: Option<Uuid>,
        captured_at_unix_ms: u64,
        event: ReplEvent,
    ) -> Self {
        Self {
            sequence,
            session_id,
            run_id,
            captured_at_unix_ms,
            event,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(super) struct ReplSessionSnapshot {
    pub session: ReplSession,
    pub last_sequence: u64,
}

#[derive(Debug, Clone)]
pub(super) struct ReplEventLog {
    session_id: Uuid,
    events: Vec<ReplEventEnvelope>,
    next_sequence: u64,
}

impl ReplEventLog {
    fn new(session_id: Uuid) -> Self {
        Self {
            session_id,
            events: Vec::new(),
            next_sequence: 1,
        }
    }

    fn append(
        &mut self,
        event: ReplEvent,
        run_id: Option<Uuid>,
        captured_at_unix_ms: u64,
    ) -> ReplEventEnvelope {
        let envelope = ReplEventEnvelope::new(
            self.next_sequence,
            self.session_id,
            run_id,
            captured_at_unix_ms,
            event,
        );
        self.next_sequence += 1;
        self.events.push(envelope.clone());
        envelope
    }

    fn events_after(&self, sequence: u64) -> Vec<ReplEventEnvelope> {
        self.events
            .iter()
            .filter(|event| event.sequence > sequence)
            .cloned()
            .collect()
    }

    fn last_sequence(&self) -> u64 {
        self.next_sequence.saturating_sub(1)
    }

    fn replay(&self, mut session: ReplSession) -> ReplSession {
        for envelope in &self.events {
            session.apply_event(&envelope.event);
        }
        session
    }
}

#[derive(Debug)]
pub(super) struct ReplEventBroker {
    log: ReplEventLog,
    sender: broadcast::Sender<ReplEventEnvelope>,
}

#[derive(Debug)]
pub(super) struct ReplEventSubscription {
    replay: std::vec::IntoIter<ReplEventEnvelope>,
    receiver: broadcast::Receiver<ReplEventEnvelope>,
}

impl ReplEventBroker {
    pub fn new(session_id: Uuid, capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity.max(1));
        Self {
            log: ReplEventLog::new(session_id),
            sender,
        }
    }

    pub fn publish(
        &mut self,
        event: ReplEvent,
        run_id: Option<Uuid>,
        captured_at_unix_ms: u64,
    ) -> ReplEventEnvelope {
        let envelope = self.log.append(event, run_id, captured_at_unix_ms);
        let _ = self.sender.send(envelope.clone());
        envelope
    }

    pub fn events_after(&self, sequence: u64) -> Vec<ReplEventEnvelope> {
        self.log.events_after(sequence)
    }

    pub fn last_sequence(&self) -> u64 {
        self.log.last_sequence()
    }

    pub fn replay(&self, session: ReplSession) -> ReplSession {
        self.log.replay(session)
    }

    pub fn subscribe_after(&self, sequence: u64) -> ReplEventSubscription {
        ReplEventSubscription {
            replay: self.log.events_after(sequence).into_iter(),
            receiver: self.sender.subscribe(),
        }
    }
}

impl ReplEventSubscription {
    pub async fn next(&mut self) -> Option<ReplEventEnvelope> {
        if let Some(event) = self.replay.next() {
            return Some(event);
        }

        loop {
            match self.receiver.recv().await {
                Ok(event) => return Some(event),
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub(super) enum ExtractionSource {
    Accessibility,
    BrowserDom,
    ScreenshotOcr,
    UserProvidedText,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub(super) enum ScreenRegionKind {
    AiOverview,
    SearchResult,
    Question,
    Choice,
    Code,
    Terminal,
    BrowserAddress,
    Documentation,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub(super) enum ScreenKind {
    Ide,
    Terminal,
    BrowserSearch,
    AssessmentMultipleChoice,
    AssessmentCode,
    Documentation,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub(super) struct BoundingBox {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(super) struct ScreenRegion {
    pub id: String,
    pub kind: ScreenRegionKind,
    pub text: String,
    pub bounding_box: BoundingBox,
    pub confidence: f32,
    pub source: ExtractionSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct CodeBlock {
    pub language: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct QuestionBlock {
    pub text: String,
    pub topic: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct TerminalBlock {
    pub command: Option<String>,
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(super) struct ScreenUnderstandingContext {
    pub source_app: Option<String>,
    pub visible_text: String,
    pub detected_kind: ScreenKind,
    pub regions: Vec<ScreenRegion>,
    pub code_blocks: Vec<CodeBlock>,
    pub question: Option<QuestionBlock>,
    pub multiple_choice_options: Vec<ScreenRegion>,
    pub terminal_blocks: Vec<TerminalBlock>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub(super) enum VoiceTurnIntent {
    OpenApplication {
        transcript: String,
        app_name: String,
    },
    OpenWebsite {
        transcript: String,
        label: String,
        url: String,
    },
    SearchWeb {
        transcript: String,
        query: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenSubjectMode {
    Explicit,
    Standalone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct KnownWebsite {
    label: &'static str,
    url: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum OpenTarget {
    Application(String),
    Website { label: String, url: String },
}

pub(super) fn resolve_voice_turn_intent(transcript: &str) -> Option<VoiceTurnIntent> {
    let transcript = transcript.trim();
    if transcript.is_empty() {
        return None;
    }

    if let Some(target) = resolve_open_target(transcript) {
        return match target {
            OpenTarget::Application(app_name) => Some(VoiceTurnIntent::OpenApplication {
                transcript: transcript.to_string(),
                app_name,
            }),
            OpenTarget::Website { label, url } => Some(VoiceTurnIntent::OpenWebsite {
                transcript: transcript.to_string(),
                label,
                url,
            }),
        };
    }

    if is_open_command_only(&normalize_transcript(transcript)) {
        return None;
    }

    let query = resolve_search_query(transcript)?;
    Some(VoiceTurnIntent::SearchWeb {
        transcript: transcript.to_string(),
        query,
    })
}

fn resolve_open_target(transcript: &str) -> Option<OpenTarget> {
    let normalized = normalize_transcript(transcript);
    if let Some(subject) = extract_open_subject(transcript, &normalized) {
        return resolve_open_subject(&subject, OpenSubjectMode::Explicit);
    }

    if is_standalone_open_candidate(&normalized) {
        return resolve_open_subject(transcript, OpenSubjectMode::Standalone);
    }

    None
}

fn extract_open_subject(raw: &str, normalized: &str) -> Option<String> {
    let prefixes = [
        "por favor abra o aplicativo",
        "por favor abra a aplicacao",
        "por favor abra o programa",
        "por favor abra o site do",
        "por favor abra o site da",
        "por favor abra o site de",
        "por favor abra o site",
        "por favor abra o",
        "por favor abra a",
        "por favor abra",
        "abra o aplicativo",
        "abra a aplicacao",
        "abra o programa",
        "abra o software",
        "abra o site do",
        "abra o site da",
        "abra o site de",
        "abra o site",
        "abra a pagina",
        "abra o",
        "abra a",
        "abra",
        "abre o aplicativo",
        "abre a aplicacao",
        "abre o programa",
        "abre o site do",
        "abre o site da",
        "abre o site de",
        "abre o site",
        "abre o",
        "abre a",
        "abre",
        "abrir o",
        "abrir a",
        "abrir",
        "acesse o",
        "acesse a",
        "acesse",
        "acessa o",
        "acessa a",
        "acessa",
        "acessar o",
        "acessar a",
        "acessar",
        "entre no",
        "entre na",
        "entre em",
        "ir para",
        "inicie o",
        "inicie a",
        "inicie",
        "execute o",
        "execute a",
        "execute",
        "abrir aplicativo",
        "open",
        "open the",
        "launch",
        "start",
    ];

    for prefix in prefixes {
        if normalized == prefix {
            return Some(String::new());
        }
        if normalized
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with(' '))
        {
            let prefix_len = prefix.chars().count();
            let start = raw
                .char_indices()
                .nth(prefix_len)
                .map(|(index, _)| index)
                .unwrap_or(raw.len());
            return Some(clean_command_subject(&raw[start..]));
        }
    }

    None
}

fn resolve_open_subject(subject: &str, mode: OpenSubjectMode) -> Option<OpenTarget> {
    let cleaned = clean_open_subject(subject)?;
    let normalized = normalize_transcript(&cleaned);

    if let Some(website) = known_website(&normalized) {
        return Some(OpenTarget::Website {
            label: website.label.to_string(),
            url: website.url.to_string(),
        });
    }

    match mode {
        OpenSubjectMode::Explicit => Some(OpenTarget::Application(cleaned)),
        OpenSubjectMode::Standalone if is_known_standalone_application(&normalized) => {
            Some(OpenTarget::Application(cleaned))
        }
        OpenSubjectMode::Standalone => None,
    }
}

fn clean_open_subject(subject: &str) -> Option<String> {
    let mut value = clean_command_subject(subject);
    if value.is_empty() {
        return None;
    }

    for qualifier in [
        "o aplicativo",
        "a aplicacao",
        "o programa",
        "o software",
        "o site do",
        "o site da",
        "o site de",
        "site do",
        "site da",
        "site de",
        "a pagina do",
        "a pagina da",
        "a pagina de",
        "pagina do",
        "pagina da",
        "pagina de",
        "aplicativo",
        "aplicacao",
        "programa",
        "software",
        "site",
        "pagina",
        "do",
        "da",
        "de",
        "o",
        "a",
        "os",
        "as",
    ] {
        let normalized = normalize_transcript(&value);
        if normalized == qualifier {
            return None;
        }
        if normalized
            .strip_prefix(qualifier)
            .is_some_and(|rest| rest.starts_with(' '))
        {
            let qualifier_len = qualifier.chars().count();
            let start = value
                .char_indices()
                .nth(qualifier_len)
                .map(|(index, _)| index)
                .unwrap_or(value.len());
            value = value[start..].trim_start().to_string();
            break;
        }
    }

    (!value.is_empty()).then_some(value)
}

fn clean_command_subject(value: &str) -> String {
    value
        .trim()
        .trim_matches(|ch| matches!(ch, '"' | '\'' | '`' | '.' | ',' | ';' | ':'))
        .to_string()
}

fn is_standalone_open_candidate(normalized: &str) -> bool {
    known_website(normalized).is_some() || is_known_standalone_application(normalized)
}

fn is_open_command_only(normalized: &str) -> bool {
    matches!(
        normalized,
        "abra"
            | "abra o"
            | "abra a"
            | "abre"
            | "abre o"
            | "abre a"
            | "abrir"
            | "acesse"
            | "acessa"
            | "acessar"
            | "entre em"
            | "ir para"
            | "inicie"
            | "execute"
            | "open"
            | "open the"
            | "launch"
            | "start"
    )
}

fn is_known_standalone_application(normalized: &str) -> bool {
    let compact = compact_normalized(normalized);
    matches!(
        compact.as_str(),
        "terminal"
            | "terminalemulator"
            | "console"
            | "shell"
            | "navegador"
            | "browser"
            | "webbrowser"
            | "firefox"
            | "chrome"
            | "chromium"
            | "brave"
            | "vscode"
            | "code"
            | "visualstudiocode"
            | "burp"
            | "burpsuite"
            | "burpsuitecommunity"
            | "wireshark"
            | "antigravity"
            | "steam"
            | "configuracoes"
            | "settings"
            | "gnomesettings"
            | "ajustes"
    )
}

fn known_website(normalized: &str) -> Option<KnownWebsite> {
    let compact = compact_normalized(normalized);
    let target = match compact.as_str() {
        "youtube" | "youtubecom" => KnownWebsite {
            label: "YouTube",
            url: "https://www.youtube.com/",
        },
        "youtubemusic" | "musicayoutube" => KnownWebsite {
            label: "YouTube Music",
            url: "https://music.youtube.com/",
        },
        "facebook" | "facebookcom" => KnownWebsite {
            label: "Facebook",
            url: "https://www.facebook.com/",
        },
        "linkedin" | "linkedincom" => KnownWebsite {
            label: "LinkedIn",
            url: "https://www.linkedin.com/",
        },
        "github" | "githubcom" => KnownWebsite {
            label: "GitHub",
            url: "https://github.com/",
        },
        "gitlab" | "gitlabcom" => KnownWebsite {
            label: "GitLab",
            url: "https://gitlab.com/",
        },
        "instagram" | "instagramcom" => KnownWebsite {
            label: "Instagram",
            url: "https://www.instagram.com/",
        },
        "reddit" | "redditcom" => KnownWebsite {
            label: "Reddit",
            url: "https://www.reddit.com/",
        },
        "stackoverflow" | "stackoverflowcom" => KnownWebsite {
            label: "Stack Overflow",
            url: "https://stackoverflow.com/",
        },
        "gmail" | "mailgoogle" | "googlemail" => KnownWebsite {
            label: "Gmail",
            url: "https://mail.google.com/",
        },
        "whatsapp" | "whatsappweb" => KnownWebsite {
            label: "WhatsApp Web",
            url: "https://web.whatsapp.com/",
        },
        "telegram" | "telegramweb" => KnownWebsite {
            label: "Telegram Web",
            url: "https://web.telegram.org/",
        },
        "google" | "googlecom" => KnownWebsite {
            label: "Google",
            url: "https://www.google.com/",
        },
        _ => return None,
    };

    Some(target)
}

fn resolve_search_query(transcript: &str) -> Option<String> {
    let stripped = strip_search_prefix(transcript);
    let query = if stripped.trim().is_empty() {
        let normalized = normalize_transcript(transcript);
        if normalized_is_search_command_only(&normalized) {
            return None;
        }
        transcript.trim()
    } else {
        stripped.as_str()
    };

    let query = clean_command_subject(query);
    (!query.is_empty()).then_some(query)
}

fn strip_search_prefix(input: &str) -> String {
    let trimmed = input.trim();
    let normalized = normalize_transcript(trimmed);
    let prefixes = [
        "pesquise por",
        "pesquise sobre",
        "pesquisar por",
        "pesquisar sobre",
        "procure por",
        "procure sobre",
        "buscar por",
        "busque por",
        "busque sobre",
        "google",
        "search for",
        "search about",
        "look up",
        "find information about",
    ];

    for prefix in prefixes {
        let prefix_len = prefix.chars().count();
        if normalized == prefix {
            return String::new();
        }
        if normalized.starts_with(prefix) {
            let start = trimmed
                .char_indices()
                .nth(prefix_len)
                .map(|(index, _)| index)
                .unwrap_or(trimmed.len());
            return trimmed[start..].trim_start().to_string();
        }
    }

    trimmed.to_string()
}

fn normalized_is_search_command_only(normalized: &str) -> bool {
    [
        "pesquise por",
        "pesquise sobre",
        "pesquisar por",
        "pesquisar sobre",
        "procure por",
        "procure sobre",
        "buscar por",
        "busque por",
        "busque sobre",
        "google",
        "search for",
        "search about",
        "look up",
        "find information about",
    ]
    .contains(&normalized)
}

fn compact_normalized(normalized: &str) -> String {
    normalized
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect()
}

fn normalize_transcript(input: &str) -> String {
    ascii_fold(input)
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch.is_ascii_whitespace() {
                ch
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn ascii_fold(input: &str) -> String {
    input
        .chars()
        .map(|ch| match ch {
            'á' | 'à' | 'ã' | 'â' | 'ä' | 'Á' | 'À' | 'Ã' | 'Â' | 'Ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => 'i',
            'ó' | 'ò' | 'õ' | 'ô' | 'ö' | 'Ó' | 'Ò' | 'Õ' | 'Ô' | 'Ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
            'ç' | 'Ç' => 'c',
            other => other.to_ascii_lowercase(),
        })
        .collect()
}
