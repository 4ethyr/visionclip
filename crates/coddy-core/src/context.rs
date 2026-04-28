use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ExtractionSource {
    Accessibility,
    BrowserDom,
    ScreenshotOcr,
    UserProvidedText,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ScreenRegionKind {
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
pub enum ScreenKind {
    Ide,
    Terminal,
    BrowserSearch,
    AssessmentMultipleChoice,
    AssessmentCode,
    Documentation,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct BoundingBox {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScreenRegion {
    pub id: String,
    pub kind: ScreenRegionKind,
    pub text: String,
    pub bounding_box: BoundingBox,
    pub confidence: f32,
    pub source: ExtractionSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeBlock {
    pub language: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuestionBlock {
    pub text: String,
    pub topic: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TerminalBlock {
    pub command: Option<String>,
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScreenUnderstandingContext {
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

impl ScreenUnderstandingContext {
    pub fn ai_overview_regions(&self) -> impl Iterator<Item = &ScreenRegion> {
        self.regions
            .iter()
            .filter(|region| region.kind == ScreenRegionKind::AiOverview)
    }
}
