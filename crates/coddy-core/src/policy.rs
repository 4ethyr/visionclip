use uuid::Uuid;

use crate::{
    AssessmentPolicy, AssistanceDecision, AssistanceFallback, RequestedHelp,
    ShortcutConflictPolicy, ShortcutDecision,
};

pub fn evaluate_assistance(
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
            | RequestedHelp::GenerateCompleteCode => AssistanceDecision::block(
                AssistanceFallback::SyntaxOnlyGuidance,
                "current policy only allows syntax-level guidance",
            ),
        },
        AssessmentPolicy::RestrictedAssessment => match requested_help {
            RequestedHelp::ExplainConcept | RequestedHelp::DebugCode => {
                AssistanceDecision::allow("conceptual help is allowed without final answers")
            }
            RequestedHelp::GenerateTests
            | RequestedHelp::SolveMultipleChoice
            | RequestedHelp::GenerateCompleteCode => AssistanceDecision::block(
                AssistanceFallback::ConceptualGuidance,
                "restricted assessments must not receive final answers or complete code",
            ),
        },
        AssessmentPolicy::UnknownAssessment => AssistanceDecision::confirm(
            AssistanceFallback::AskForPolicyConfirmation,
            "assessment policy is unknown and requires confirmation",
        ),
    }
}

pub fn evaluate_shortcut_conflict(
    policy: ShortcutConflictPolicy,
    active_run_id: Option<Uuid>,
) -> ShortcutDecision {
    let Some(previous_run_id) = active_run_id else {
        return ShortcutDecision::StartListening {
            run_id: Uuid::new_v4(),
        };
    };

    match policy {
        ShortcutConflictPolicy::IgnoreWhileBusy => ShortcutDecision::IgnoredBusy {
            active_run_id: previous_run_id,
        },
        ShortcutConflictPolicy::StopSpeakingAndListen => ShortcutDecision::StoppedSpeaking {
            previous_run_id,
            next_run_id: Uuid::new_v4(),
        },
        ShortcutConflictPolicy::CancelRunAndListen => ShortcutDecision::CancelledRun {
            previous_run_id,
            next_run_id: Uuid::new_v4(),
        },
    }
}
