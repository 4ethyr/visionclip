use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum IntentKind {
    OpenApplication,
    SearchWeb,
    AskKnowledge,
    ExplainSearchResult,
    ReadScreen,
    SummarizeScreen,
    SystemCommand,
    FileSearch,
    Clarification,
    Unknown,
}

impl IntentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            IntentKind::OpenApplication => "OpenApplicationIntent",
            IntentKind::SearchWeb => "SearchWebIntent",
            IntentKind::AskKnowledge => "AskKnowledgeIntent",
            IntentKind::ExplainSearchResult => "ExplainSearchResultIntent",
            IntentKind::ReadScreen => "ReadScreenIntent",
            IntentKind::SummarizeScreen => "SummarizeScreenIntent",
            IntentKind::SystemCommand => "SystemCommandIntent",
            IntentKind::FileSearch => "FileSearchIntent",
            IntentKind::Clarification => "ClarificationIntent",
            IntentKind::Unknown => "UnknownIntent",
        }
    }

    pub fn minimum_confidence(self) -> f32 {
        match self {
            IntentKind::OpenApplication => 0.78,
            IntentKind::SearchWeb => 0.72,
            IntentKind::AskKnowledge => 0.70,
            IntentKind::ExplainSearchResult => 0.78,
            IntentKind::ReadScreen | IntentKind::SummarizeScreen => 0.80,
            IntentKind::SystemCommand | IntentKind::FileSearch => 0.86,
            IntentKind::Clarification => 0.0,
            IntentKind::Unknown => 0.0,
        }
    }
}

impl std::str::FromStr for IntentKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "openapplicationintent" | "open_application" | "openapplication" => {
                Ok(IntentKind::OpenApplication)
            }
            "searchwebintent" | "search_web" | "searchweb" => Ok(IntentKind::SearchWeb),
            "askknowledgeintent" | "ask_knowledge" | "askknowledge" => Ok(IntentKind::AskKnowledge),
            "explainsearchresultintent" | "explain_search_result" => {
                Ok(IntentKind::ExplainSearchResult)
            }
            "readscreeenintent" | "readscreenintent" | "read_screen" | "readscreen" => {
                Ok(IntentKind::ReadScreen)
            }
            "summarizescreenintent" | "summarize_screen" | "summarizescreen" => {
                Ok(IntentKind::SummarizeScreen)
            }
            "systemcommandintent" | "system_command" | "systemcommand" => {
                Ok(IntentKind::SystemCommand)
            }
            "filesearchintent" | "file_search" | "filesearch" => Ok(IntentKind::FileSearch),
            "clarificationintent" | "clarification" => Ok(IntentKind::Clarification),
            "unknownintent" | "unknown" => Ok(IntentKind::Unknown),
            other => Err(format!("unknown intent kind: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IntentDetection {
    pub intent: IntentKind,
    pub confidence: f32,
    #[serde(default)]
    pub slots: Map<String, Value>,
    pub raw_text: String,
    #[serde(default)]
    pub normalized_text: String,
    #[serde(default)]
    pub language: Option<String>,
}

impl IntentDetection {
    pub fn is_confident(&self) -> bool {
        self.confidence >= self.intent.minimum_confidence()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_names_match_agent_contract() {
        assert_eq!(
            IntentKind::OpenApplication.as_str(),
            "OpenApplicationIntent"
        );
        assert_eq!(IntentKind::SearchWeb.as_str(), "SearchWebIntent");
    }

    #[test]
    fn confidence_thresholds_drive_clarification() {
        let detection = IntentDetection {
            intent: IntentKind::OpenApplication,
            confidence: 0.5,
            slots: Map::new(),
            raw_text: "abra o code".into(),
            normalized_text: "abra o code".into(),
            language: Some("pt-BR".into()),
        };

        assert!(!detection.is_confident());
    }
}
