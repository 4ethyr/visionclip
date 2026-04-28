use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AssessmentPolicy {
    Practice,
    PermittedAi,
    SyntaxOnly,
    RestrictedAssessment,
    UnknownAssessment,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum RequestedHelp {
    ExplainConcept,
    SolveMultipleChoice,
    GenerateCompleteCode,
    DebugCode,
    GenerateTests,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AssistanceFallback {
    None,
    ConceptualGuidance,
    SyntaxOnlyGuidance,
    AskForPolicyConfirmation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssistanceDecision {
    pub allowed: bool,
    pub requires_confirmation: bool,
    pub fallback: AssistanceFallback,
    pub reason: String,
}

impl AssistanceDecision {
    pub fn allow(reason: impl Into<String>) -> Self {
        Self {
            allowed: true,
            requires_confirmation: false,
            fallback: AssistanceFallback::None,
            reason: reason.into(),
        }
    }

    pub fn block(fallback: AssistanceFallback, reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            requires_confirmation: false,
            fallback,
            reason: reason.into(),
        }
    }

    pub fn confirm(fallback: AssistanceFallback, reason: impl Into<String>) -> Self {
        Self {
            allowed: false,
            requires_confirmation: true,
            fallback,
            reason: reason.into(),
        }
    }
}
