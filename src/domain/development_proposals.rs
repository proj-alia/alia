//! Preliminary model-bound checks. The transaction revalidates the real head,
//! source classification and privacy state before writing only a proposal.
use super::{
    cognition::{ModelContext, ProposalError},
    config::PersonaSeed,
    development::{DevelopmentDraft, DevelopmentTarget, StrategyScope, TaskStrategy},
    history::SourceKind,
};
use std::collections::HashSet;

pub(crate) fn validate(
    context: &ModelContext,
    draft: &DevelopmentDraft,
) -> Result<(), ProposalError> {
    let invalid = || ProposalError::Development;
    let scope = if draft.target == DevelopmentTarget::TaskStrategy {
        let strategy: TaskStrategy =
            serde_json::from_value(draft.proposed_value.clone()).map_err(|_| invalid())?;
        if strategy.description.trim().is_empty()
            || strategy.description.contains('\0')
            || strategy.description.chars().count() > 2000
            || !match strategy.scope {
                StrategyScope::Task(id) => id == context.run_id,
                StrategyScope::Goal(id) => Some(id) == context.current_goal_id,
            }
        {
            return Err(invalid());
        }
        Some(strategy.scope)
    } else {
        None
    };
    let target = context
        .development_targets
        .iter()
        .find(|t| {
            t.target == draft.target && t.scope == scope && t.before_version == draft.before_version
        })
        .ok_or_else(invalid)?;
    if draft.before_version <= 0
        || draft.deadline <= context.now
        || draft.proposed_description.trim().is_empty()
        || draft.validation_plan.trim().is_empty()
        || [&draft.proposed_description, &draft.validation_plan]
            .iter()
            .any(|s| s.contains('\0'))
        || target
            .target_id
            .iter()
            .chain(target.head_revision_id.iter())
            .any(|id| !context.input_manifest.contains(id))
    {
        return Err(invalid());
    }
    let mut seen = HashSet::new();
    let mut external_support = false;
    for (ids, supporting) in [
        (&draft.evidence_ids, true),
        (&draft.counterevidence_ids, false),
    ] {
        for id in ids {
            let event = context
                .events
                .iter()
                .find(|e| e.id == *id)
                .ok_or_else(invalid)?;
            if id.is_nil()
                || !seen.insert(*id)
                || !context.input_manifest.contains(id)
                || event.individual_id != context.individual_id
                || event.recorded_at > context.now
                || event.source_kind == SourceKind::FictionalBackground
                || event.kind == "decision.reviewed"
            {
                return Err(invalid());
            }
            external_support |= supporting
                && matches!(
                    event.source_kind,
                    SourceKind::DirectObservation
                        | SourceKind::ReportedClaim
                        | SourceKind::ExternalDocument
                )
                && event.kind != "individual.created"
                && !event.kind.starts_with("affect.");
        }
    }
    if !external_support {
        return Err(invalid());
    }
    if let Some(pointer) = draft.target.persona_pointer() {
        let mut after = context.persona.clone();
        let before = after.pointer_mut(pointer).ok_or_else(invalid)?;
        if *before == draft.proposed_value {
            return Err(invalid());
        }
        *before = draft.proposed_value.clone();
        PersonaSeed::validate(after).map_err(|_| invalid())?;
    } else if context
        .development_context
        .as_ref()
        .and_then(|d| d.effective_task_strategy.as_ref())
        .is_some_and(|s| {
            Some(&s.scope) == scope.as_ref()
                && serde_json::to_value(s).ok().as_ref() == Some(&draft.proposed_value)
        })
    {
        return Err(invalid());
    }
    Ok(())
}
