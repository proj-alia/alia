use super::{
    goals::unavailable,
    store::{StoreError, digest},
};
use crate::domain::completion::{Citation, CompletionContract, covers};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;
type Tx<'a> = Transaction<'a, Postgres>;

/// Validate the actual source text and previously observed coverage, including
/// previous cycles of the same goal. Unrelated runs cannot lend their experience.
#[derive(Clone, Copy)]
pub(super) enum CitationFailure {
    Unverified,
    QuoteMissing,
}
impl CitationFailure {
    pub fn feedback(self) -> (&'static str, &'static str) {
        match self {
            Self::Unverified => (
                "citation_not_observed_or_mismatched",
                "引用必须对应当前运行、该目标的形成运行或同目标先前循环真实读过且仍有效的完整范围。",
            ),
            Self::QuoteMissing => (
                "citation_quote_missing",
                "已核实读取范围，但笔记正文缺少此范围的完整连续原文。将读取结果text逐字保留后另写分析；不能删句、改字或用分散引句代替整段。也可以重新读取更短片段并引用它返回的citation。",
            ),
        }
    }
}
pub(super) async fn citation_sources(
    tx: &mut Tx<'_>,
    individual: Uuid,
    run: Uuid,
    action: Uuid,
    citation: &Citation,
    note: &str,
) -> Result<Option<Vec<Uuid>>, StoreError> {
    Ok(check_citation(tx, individual, run, action, citation, note)
        .await?
        .ok())
}

pub(super) async fn check_citation(
    tx: &mut Tx<'_>,
    individual: Uuid,
    run: Uuid,
    action: Uuid,
    citation: &Citation,
    note: &str,
) -> Result<Result<Vec<Uuid>, CitationFailure>, StoreError> {
    if !citation.valid() || unavailable(tx, individual, &[citation.document_id]).await? {
        return Ok(Err(CitationFailure::Unverified));
    }
    let body: Option<String> =
        sqlx::query_scalar("SELECT body FROM documents WHERE individual_id=$1 AND event_id=$2")
            .bind(individual)
            .bind(citation.document_id)
            .fetch_optional(&mut **tx)
            .await?;
    let Some(body) = body else {
        return Ok(Err(CitationFailure::Unverified));
    };
    let Some(quote) = citation.excerpt(&body) else {
        return Ok(Err(CitationFailure::Unverified));
    };
    let reads:Vec<(Uuid,Value)>=sqlx::query_as(r#"SELECT e.id,e.payload FROM actions a
 JOIN runs r ON (r.individual_id,r.id)=(a.individual_id,a.run_id)
 JOIN runs target ON target.individual_id=r.individual_id AND target.id=$2
 JOIN actions writing ON writing.individual_id=target.individual_id AND writing.id=$3 AND writing.run_id=target.id
 JOIN individuals i ON i.id=r.individual_id
 JOIN events e ON (e.individual_id,e.id)=(a.individual_id,a.result_event_id)
 WHERE a.individual_id=$1 AND a.status='succeeded' AND a.capability_id IN ('documents.read','web.read_allowed')
 AND r.privacy_epoch=i.privacy_epoch
 AND ((r.id=target.id AND a.sequence<writing.sequence) OR (target.goal_id IS NOT NULL AND (r.goal_id=target.goal_id OR r.id=(SELECT formed_in_run FROM goals WHERE individual_id=$1 AND id=target.goal_id)) AND r.created_at<target.created_at))
 AND e.payload->'output'->>'document_id'=$4
 ORDER BY e.sequence"#).bind(individual).bind(run).bind(action).bind(citation.document_id.to_string()).fetch_all(&mut **tx).await?;
    let mut ranges = Vec::new();
    let mut sources = vec![citation.document_id];
    let chars: Vec<char> = body.chars().collect();
    for (id, payload) in reads {
        if unavailable(tx, individual, &[id]).await? {
            continue;
        }
        let Some(text) = payload["output"]["text"].as_str() else {
            continue;
        };
        let start = payload["output"]["offset"].as_u64().unwrap_or(0) as usize;
        let Some(end) = start.checked_add(text.chars().count()) else {
            continue;
        };
        if end > chars.len()
            || start >= end
            || end <= citation.start_char
            || start >= citation.end_char
        {
            continue;
        }
        if chars[start..end].iter().copied().ne(text.chars()) {
            continue;
        }
        ranges.push((start, end));
        sources.push(id);
    }
    if !covers(citation, ranges) {
        return Ok(Err(CitationFailure::Unverified));
    }
    if !note.contains(&quote) {
        return Ok(Err(CitationFailure::QuoteMissing));
    }
    Ok(Ok(sources))
}

#[derive(Clone, Serialize)]
pub(super) struct VerifiedCompletion {
    pub checker_version: &'static str,
    pub contract_digest: String,
    pub contract: Option<CompletionContract>,
    pub note_action_id: Uuid,
    pub result_event_id: Uuid,
    pub evidence_ids: Vec<Uuid>,
}
type NoteCandidate = (Uuid, Uuid, Uuid, String, Value);
/// Read-only verification: no completion flag supplied by the model is trusted.
pub(super) async fn verify(
    tx: &mut Tx<'_>,
    individual: Uuid,
    goal: Uuid,
) -> Result<Option<VerifiedCompletion>, StoreError> {
    if unavailable(tx, individual, &[individual, goal]).await? {
        return Ok(None);
    }
    let row:Option<(Value,String)>=sqlx::query_as("SELECT completion,success_criterion FROM goals WHERE individual_id=$1 AND id=$2 AND status='active' AND completion IS NOT NULL").bind(individual).bind(goal).fetch_optional(&mut **tx).await?;
    let Some((raw, criterion)) = row else {
        return super::reviews::verify(tx, individual, goal).await;
    };
    let contract: CompletionContract = serde_json::from_value(raw)?;
    if !contract.valid_for(&criterion) {
        return Ok(None);
    }
    let citation = contract.citation();
    let notes:Vec<NoteCandidate>=sqlx::query_as("SELECT a.id,a.run_id,a.result_event_id,n.body,n.citations FROM notes n JOIN actions a ON (a.individual_id,a.id)=(n.individual_id,n.action_id) JOIN runs r ON (r.individual_id,r.id)=(a.individual_id,a.run_id) JOIN individuals i ON i.id=r.individual_id WHERE r.individual_id=$1 AND (r.goal_id=$2 OR r.id=(SELECT formed_in_run FROM goals WHERE individual_id=$1 AND id=$2)) AND a.status='succeeded' AND r.privacy_epoch=i.privacy_epoch ORDER BY n.created_at DESC,a.id")
        .bind(individual).bind(goal).fetch_all(&mut **tx).await?;
    for (action, run, result, body, citations) in notes {
        let citations: Vec<Citation> = serde_json::from_value(citations)?;
        if !citations.contains(&citation) || unavailable(tx, individual, &[action, result]).await? {
            continue;
        }
        if let Some(mut evidence) =
            citation_sources(tx, individual, run, action, &citation, &body).await?
        {
            evidence.extend([action, result]);
            evidence.sort();
            evidence.dedup();
            return Ok(Some(VerifiedCompletion {
                checker_version: "source_excerpt/2",
                contract_digest: digest(&json!({"contract":contract,"criterion":criterion}))?,
                contract: Some(contract),
                note_action_id: action,
                result_event_id: result,
                evidence_ids: evidence,
            }));
        }
    }
    Ok(None)
}
