use super::{
    scheduling::require_lease,
    store::{
        Store, StoreError, advance, apply_memories, digest, edge, insert_event, object, outbox,
        state,
    },
};
use crate::{
    domain::cognition::{CapabilityInfo, GoalChange, GoalOriginRef, ProposalError, RunMode},
    runtime::{
        execution::PreparedDecision,
        scheduling::{DecisionReceipt, RunLease, TokenAllowance},
    },
};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

async fn provenance(
    tx: &mut Transaction<'_, Postgres>,
    individual: Uuid,
    inputs: &[Uuid],
    target: Uuid,
) -> Result<(), StoreError> {
    for source in inputs {
        if *source != target {
            edge(tx, individual, *source, target).await?;
        }
    }
    Ok(())
}

impl Store {
    /// One atomic state transition, including all memories, goals, action intents,
    /// reply/draft, provenance and outbox. Only a charged, prepared decision enters.
    pub async fn commit_decision(
        &self,
        lease: &RunLease,
        prepared: &PreparedDecision,
        registry: &[CapabilityInfo],
    ) -> Result<DecisionReceipt, StoreError> {
        let context = &prepared.context;
        let proposal = &prepared.result.proposal;
        proposal.validate_context(context)?;
        let hash = digest(proposal)?;
        let mut tx = self.write().await?;
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        super::scheduling::require_regular_run(&mut tx, lease.individual_id, lease.run_id).await?;
        let previous:Option<(Uuid,Uuid,i64,i64,String,Value,Uuid)>=sqlx::query_as("SELECT individual_id,run_id,lease_generation,privacy_epoch,proposal_digest,receipt,decision_event_id FROM decision_commits WHERE reservation_id=$1")
            .bind(prepared.reservation_id).fetch_optional(&mut *tx).await?;
        if let Some((individual, run, generation, epoch, previous, receipt, event)) = previous {
            if individual != lease.individual_id
                || run != lease.run_id
                || generation != lease.generation
                || previous != hash
            {
                return Err(StoreError::IdempotencyConflict);
            }
            if current.privacy_epoch != epoch {
                return Err(StoreError::StaleState);
            }
            let receipt: DecisionReceipt = serde_json::from_value(receipt)?;
            if receipt.decision_event_id != event {
                return Err(StoreError::StaleState);
            }
            // Check the actual post-author proof as well as the input snapshot:
            // it was created after that snapshot and is not in its manifest.
            let review: Option<Uuid> = sqlx::query_scalar(
                "SELECT event_id FROM decision_reviews WHERE author_reservation_id=$1 AND individual_id=$2 AND run_id=$3",
            )
            .bind(prepared.reservation_id)
            .bind(individual)
            .bind(run)
            .fetch_optional(&mut *tx)
            .await?;
            if review != prepared.grounding_review_id() {
                return Err(StoreError::StaleState);
            }
            // A receipt is private derived data too. Recheck every returned
            // object and proof even if general derivation edges are absent.
            let mut references = context.input_manifest.clone();
            references.extend([individual, event]);
            references.extend(review);
            references.extend(&receipt.memory_ids);
            references.extend(&receipt.belief_ids);
            references.extend(&receipt.autobiography_ids);
            references.extend(&receipt.appraisal_ids);
            references.extend(&receipt.attitude_ids);
            references.extend(&receipt.relationship_revision_ids);
            references.extend(&receipt.development_ids);
            references.extend(&receipt.goal_ids);
            references.extend(&receipt.action_ids);
            references.extend(receipt.message_id);
            super::development_context::require_sources(&mut tx, individual, &references).await?;
            if super::goals::unavailable(&mut tx, individual, &references).await? {
                return Err(StoreError::StaleState);
            }
            return Ok(receipt);
        }
        if context
            .social_context
            .as_ref()
            .is_some_and(|s| !s.valid_for(context) || !s.current_at(chrono::Utc::now()))
        {
            return Err(StoreError::StaleState);
        }
        require_lease(&mut tx, lease).await?;
        super::development_context::require_context(&mut tx, context).await?;
        if context.individual_id != lease.individual_id
            || context.run_id != lease.run_id
            || context.state_version != lease.state_version
            || context.privacy_epoch != lease.privacy_epoch
            || context.config_version != lease.config_version
            || context.mode != lease.mode
        {
            return Err(StoreError::StaleState);
        }
        let frozen: bool = sqlx::query_scalar("SELECT budget_frozen FROM individuals WHERE id=$1")
            .bind(current.id)
            .fetch_one(&mut *tx)
            .await?;
        if frozen {
            return Err(StoreError::BudgetUnavailable);
        }
        let accounting:Option<(String,i64,i64,i64)>=sqlx::query_as("SELECT request_digest,input_bound,output_cap,lease_generation FROM usage_reservations WHERE id=$1 AND individual_id=$2 AND run_id=$3 AND status='settled' AND dispatched_at IS NOT NULL")
            .bind(prepared.reservation_id).bind(current.id).bind(lease.run_id).fetch_optional(&mut *tx).await?;
        let (expected, input, output, generation) =
            accounting.ok_or(StoreError::Invalid("unsettled model decision"))?;
        let actual = digest(
            &json!({"context":context,"allowance":TokenAllowance{input_upper_bound:input as u64,output_cap:output as u32}}),
        )?;
        if expected != actual || generation != lease.generation {
            return Err(StoreError::StaleState);
        }
        let grounding = Store::require_decision_review(&mut tx, lease, prepared).await?;
        let sources: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM objects WHERE individual_id=$1 AND id=ANY($2) AND NOT blocked",
        )
        .bind(current.id)
        .bind(&context.input_manifest)
        .fetch_one(&mut *tx)
        .await?;
        if sources != context.input_manifest.len() as i64 {
            return Err(StoreError::StaleState);
        }
        let unfinished:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM actions WHERE run_id=$1 AND status IN ('planned','executing','unknown')) OR EXISTS(SELECT 1 FROM usage_reservations WHERE run_id=$1 AND status IN ('reserved','dispatched'))")
            .bind(lease.run_id).fetch_one(&mut *tx).await?;
        if unfinished {
            return Err(StoreError::Invalid(
                "previous action or model call unfinished",
            ));
        }
        // Revalidate actual registry availability, version, and saved permissions.
        for intent in &proposal.tool_intents {
            let advertised = context
                .capabilities
                .iter()
                .find(|c| c.id == intent.capability_id)
                .ok_or(ProposalError::Capability)?;
            let allowed = current.runtime_config["capabilities"]["enabled"]
                .as_array()
                .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(&intent.capability_id)));
            if !allowed
                || !registry.iter().any(|c| {
                    c.id == intent.capability_id
                        && c.version == advertised.version
                        && c.available
                        && c.authorized
                })
            {
                return Err(ProposalError::Capability.into());
            }
        }
        let used: i32 = sqlx::query_scalar("SELECT tool_calls FROM runs WHERE id=$1")
            .bind(lease.run_id)
            .fetch_one(&mut *tx)
            .await?;
        let cap = current.runtime_config["autonomy"]["max_tool_calls_per_cycle"]
            .as_u64()
            .ok_or(StoreError::Invalid("tool limit"))?;
        if used as u64 + proposal.tool_intents.len() as u64 > cap {
            return Err(StoreError::CallLimit);
        }
        let version = current.state_version + 1;
        let decision = Uuid::new_v4();
        insert_event(&mut tx,current.id,decision,"decision.accepted","generated","alia",json!({"brief_reason":proposal.brief_reason,"reply_evidence_ids":proposal.reply_evidence_ids,"provider_model":prepared.result.provider_model,"provider_mode":prepared.result.provider_mode,"grounding_review_id":grounding,"grounding_status":if grounding.is_some(){"supported"}else if crate::domain::grounding::targets(proposal).is_empty(){"not_required"}else{"fixture_unchecked"}}),lease.run_id,None).await?;
        if let Some(review) = grounding {
            edge(&mut tx, current.id, review, decision).await?;
        }
        provenance(&mut tx, current.id, &context.input_manifest, decision).await?;
        let beliefs = super::beliefs::apply_model_changes(
            &mut tx,
            context,
            &proposal.belief_changes,
            decision,
            version,
        )
        .await?;
        let autobiographies = super::autobiography::apply_model_changes(
            &mut tx,
            context,
            &proposal.autobiography_changes,
            decision,
            version,
        )
        .await?;
        let affect = super::affect::apply_model_changes(
            &mut tx,
            context,
            &proposal.appraisal_changes,
            &proposal.attitude_changes,
            decision,
            version,
        )
        .await?;
        let relationship_revisions = super::relationship_revisions::apply_model_changes(
            &mut tx,
            context,
            &proposal.relationship_changes,
            decision,
            version,
        )
        .await?;
        let mut developments = Vec::new();
        if let Some(draft) = &proposal.development {
            let proposed = super::development::propose_in_tx(
                &mut tx,
                current.id,
                &context.input_manifest,
                current.id,
                draft,
                None,
            )
            .await?;
            edge(&mut tx, current.id, decision, proposed.development_id).await?;
            edge(&mut tx, current.id, decision, proposed.event_id).await?;
            outbox(
                &mut tx,
                current.id,
                "development.changed",
                proposed.development_id,
                version,
            )
            .await?;
            developments.push(proposed.development_id);
        }
        let memories = apply_memories(
            &mut tx,
            current.id,
            &context.input_manifest,
            &proposal.memory_changes,
        )
        .await?;
        for id in &memories {
            edge(&mut tx, current.id, decision, *id).await?;
            outbox(&mut tx, current.id, "memory.changed", *id, version).await?;
        }
        super::entities::apply_model_proposals(
            &mut tx,
            current.id,
            decision,
            version,
            &proposal.entity_changes,
            &memories,
            context,
        )
        .await?;
        let mut goals = Vec::new();
        for change in &proposal.goal_changes {
            let verified = if let GoalChange::Complete {
                goal_id,
                evidence_ids,
                ..
            } = change
            {
                let proof = super::completion::verify(&mut tx, current.id, *goal_id)
                    .await?
                    .ok_or(ProposalError::UnverifiedCompletion)?;
                if !evidence_ids.contains(&proof.result_event_id) {
                    return Err(ProposalError::CompletionEvidenceMissing.into());
                }
                Some(proof)
            } else {
                None
            };
            let (id, previous_status, next_status, reason) = match change {
                GoalChange::Create {
                    origins,
                    description,
                    success_criterion,
                    next_step,
                    stop_condition,
                    priority,
                    completion,
                } => {
                    for origin in origins {
                        if let GoalOriginRef::Interest(id) = origin {
                            let object =
                                super::interests::active_object(&mut tx, current.id, id).await?;
                            if !context.input_manifest.contains(&object) {
                                return Err(StoreError::StaleState);
                            }
                        }
                    }
                    let (origin_type, origin_ids) = GoalOriginRef::storage_projection(origins);
                    if GoalOriginRef::from_storage(origin_type, &origin_ids)? != *origins {
                        return Err(ProposalError::GoalOrigin.into());
                    }
                    // Changing the source label cannot clone an exhausted cause.
                    // A different actual observation can support a new goal even
                    // when it shares a general interest with the stopped one.
                    let event_keys: Vec<_> = origins
                        .iter()
                        .filter(|o| matches!(o, GoalOriginRef::Event(_)))
                        .collect();
                    let keys: Vec<_> = if event_keys.is_empty() {
                        origins.iter().collect()
                    } else {
                        event_keys
                    };
                    let stopped:Vec<(String,Value)>=sqlx::query_as("WITH RECURSIVE denied(id) AS (SELECT id FROM objects WHERE individual_id=$1 AND blocked UNION SELECT e.target_id FROM derivation_edges e JOIN denied d ON d.id=e.source_id WHERE e.individual_id=$1) SELECT g.origin_type,g.origin_ids FROM goals g WHERE g.individual_id=$1 AND g.status='waiting' AND g.waiting_reason='no_progress' AND NOT EXISTS(SELECT 1 FROM denied WHERE id=g.id)")
                        .bind(current.id).fetch_all(&mut *tx).await?;
                    let mut halted = false;
                    for (kind, ids) in stopped {
                        let ids: Vec<String> = serde_json::from_value(ids)?;
                        let previous = GoalOriginRef::from_storage(&kind, &ids)
                            .map_err(|_| StoreError::Invalid("stored goal origins"))?;
                        if previous.iter().any(|old| keys.contains(&old)) {
                            halted = true;
                            break;
                        }
                    }
                    if halted {
                        return Err(StoreError::Invalid(
                            "resume the existing goal with new evidence",
                        ));
                    }
                    if let Some(contract) = completion {
                        let citation = contract.citation();
                        if super::goals::unavailable(&mut tx, current.id, &[citation.document_id])
                            .await?
                        {
                            return Err(StoreError::StaleState);
                        }
                        let body: Option<String> = sqlx::query_scalar(
                            "SELECT body FROM documents WHERE individual_id=$1 AND event_id=$2",
                        )
                        .bind(current.id)
                        .bind(citation.document_id)
                        .fetch_optional(&mut *tx)
                        .await?;
                        if body.as_deref().and_then(|b| citation.excerpt(b)).is_none() {
                            return Err(ProposalError::Goal.into());
                        }
                    }
                    let id = Uuid::new_v4();
                    object(&mut tx, current.id, id, "goal").await?;
                    sqlx::query("INSERT INTO goals(individual_id,id,origin_type,origin_ids,description,success_criterion,next_step,stop_condition,priority,completion,due_at,formed_in_run) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,clock_timestamp(),$11)")
                        .bind(current.id).bind(id).bind(origin_type).bind(json!(origin_ids)).bind(description).bind(success_criterion).bind(next_step).bind(stop_condition).bind(i32::from(*priority)).bind(completion.as_ref().map(serde_json::to_value).transpose()?).bind(lease.run_id).execute(&mut *tx).await?;
                    (id, None, "proposed", None)
                }
                GoalChange::Activate {
                    goal_id, reason, ..
                }
                | GoalChange::Wait {
                    goal_id, reason, ..
                }
                | GoalChange::Abandon {
                    goal_id, reason, ..
                }
                | GoalChange::Complete {
                    goal_id, reason, ..
                } => {
                    let expected = context
                        .goals
                        .iter()
                        .find(|g| g.id == *goal_id)
                        .ok_or(ProposalError::Goal)?;
                    let next = match change {
                        GoalChange::Activate { .. } => "active",
                        GoalChange::Wait { .. } => "waiting",
                        GoalChange::Complete { .. } => "completed",
                        _ => "abandoned",
                    };
                    let mut reset_no_progress = false;
                    if let GoalChange::Activate { evidence_ids, .. } = change {
                        let needs_evidence: bool=sqlx::query_scalar("SELECT COALESCE(waiting_reason='no_progress',false) OR no_progress_cycles>=3 FROM goals WHERE individual_id=$1 AND id=$2").bind(current.id).bind(goal_id).fetch_one(&mut *tx).await?;
                        reset_no_progress = needs_evidence;
                        if needs_evidence {
                            let fresh: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM events e JOIN objects o ON (o.individual_id,o.id)=(e.individual_id,e.id) JOIN goals g ON g.individual_id=e.individual_id WHERE g.id=$2 AND e.individual_id=$1 AND e.id=ANY($3) AND e.sequence>g.waiting_since_sequence AND NOT o.blocked AND (e.kind IN ('message.received','document.imported') OR (e.kind='tool.result' AND e.source_kind='external_document')))").bind(current.id).bind(goal_id).bind(evidence_ids).fetch_one(&mut *tx).await?;
                            if !fresh
                                || super::goals::unavailable(&mut tx, current.id, evidence_ids)
                                    .await?
                            {
                                return Err(StoreError::Invalid(
                                    "goal needs new external evidence",
                                ));
                            }
                        }
                    }
                    let result=sqlx::query("UPDATE goals SET status=$3,version=version+1,review_revision=review_revision+1,waiting_reason=$4,resume_at=NULL,due_at=CASE WHEN $3='active' THEN clock_timestamp() ELSE NULL END WHERE individual_id=$1 AND id=$2 AND status=$5")
                        .bind(current.id).bind(goal_id).bind(next).bind(if next=="waiting"{Some(reason)}else{None}).bind(&expected.status).execute(&mut *tx).await?;
                    if result.rows_affected() != 1 {
                        return Err(StoreError::StaleState);
                    }
                    if next == "active" {
                        sqlx::query("UPDATE goals SET no_progress_cycles=CASE WHEN $3 THEN 0 ELSE no_progress_cycles END,waiting_since_sequence=NULL WHERE individual_id=$1 AND id=$2").bind(current.id).bind(goal_id).bind(reset_no_progress).execute(&mut *tx).await?;
                    }
                    (
                        *goal_id,
                        Some(expected.status.as_str()),
                        next,
                        Some(reason.as_str()),
                    )
                }
            };
            if let Some(proof) = verified {
                let check = Uuid::new_v4();
                insert_event(
                    &mut tx,
                    current.id,
                    check,
                    "goal.completion_verified",
                    if proof.checker_version == "source_excerpt/2" {
                        "direct_observation"
                    } else {
                        "inference"
                    },
                    "runtime",
                    json!({"goal_id":id,"proof":proof}),
                    lease.run_id,
                    None,
                )
                .await?;
                provenance(&mut tx, current.id, &proof.evidence_ids, check).await?;
                edge(&mut tx, current.id, id, check).await?;
                edge(&mut tx, current.id, check, id).await?;
            }
            let event = Uuid::new_v4();
            insert_event(&mut tx,current.id,event,"goal.changed","generated","alia",json!({"goal_id":id,"from":previous_status,"to":next_status,"reason":reason,"change":change,"decision_id":decision}),lease.run_id,None).await?;
            provenance(&mut tx, current.id, &context.input_manifest, id).await?;
            provenance(&mut tx, current.id, &context.input_manifest, event).await?;
            edge(&mut tx, current.id, decision, id).await?;
            edge(&mut tx, current.id, decision, event).await?;
            outbox(&mut tx, current.id, "goal.changed", id, version).await?;
            goals.push(id);
        }
        let mut actions = Vec::new();
        let mut sequence: i32 =
            sqlx::query_scalar("SELECT COALESCE(max(sequence),0) FROM actions WHERE run_id=$1")
                .bind(lease.run_id)
                .fetch_one(&mut *tx)
                .await?;
        for intent in &proposal.tool_intents {
            sequence += 1;
            let id = Uuid::new_v4();
            object(&mut tx, current.id, id, "action").await?;
            let capability = registry
                .iter()
                .find(|c| c.id == intent.capability_id)
                .ok_or(ProposalError::Capability)?;
            sqlx::query("INSERT INTO actions(individual_id,id,run_id,decision_event_id,sequence,capability_id,capability_version,arguments) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
                .bind(current.id).bind(id).bind(lease.run_id).bind(decision).bind(sequence).bind(&intent.capability_id).bind(&capability.version).bind(&intent.input).execute(&mut *tx).await?;
            provenance(&mut tx, current.id, &context.input_manifest, id).await?;
            edge(&mut tx, current.id, decision, id).await?;
            actions.push(id);
        }
        let mut message_id = None;
        let mut message_status = None;
        if !proposal.reply.trim().is_empty() {
            let id = Uuid::new_v4();
            let foreground = lease.mode == RunMode::Foreground;
            let kind = if foreground {
                "message.sent"
            } else {
                "message.drafted"
            };
            let status = if foreground { "delivered" } else { "pending" };
            let conversation = context
                .events
                .iter()
                .find(|e| e.id == context.current_event_id)
                .and_then(|e| e.payload.get("conversation_id"))
                .cloned()
                .unwrap_or(Value::Null);
            insert_event(&mut tx,current.id,id,kind,"generated","alia",json!({"text":proposal.reply,"evidence_ids":proposal.reply_evidence_ids,"conversation_id":conversation,"decision_id":decision}),lease.run_id,None).await?;
            provenance(&mut tx, current.id, &context.input_manifest, id).await?;
            edge(&mut tx, current.id, decision, id).await?;
            sqlx::query("INSERT INTO outgoing_messages(individual_id,draft_event_id,run_id,mode,privacy_epoch,status,delivered_event_id,delivered_at) VALUES($1,$2,$3,$4,$5,$6,$7,CASE WHEN $6='delivered' THEN clock_timestamp() ELSE NULL END)")
                .bind(current.id).bind(id).bind(lease.run_id).bind(if foreground{"foreground"}else{"background"}).bind(current.privacy_epoch).bind(status).bind(if foreground{Some(id)}else{None}).execute(&mut *tx).await?;
            if foreground {
                let subject:Uuid=sqlx::query_scalar("INSERT INTO relationship_events(individual_id,subject_id,event_id) SELECT individual_id,subject_id,$2 FROM relationships WHERE individual_id=$1 AND subject_key='owner' RETURNING subject_id").bind(current.id).bind(id).fetch_one(&mut *tx).await?;
                edge(&mut tx, current.id, id, subject).await?;
                sqlx::query("UPDATE relationships SET version=version+1 WHERE individual_id=$1 AND subject_key='owner'").bind(current.id).execute(&mut *tx).await?;
                outbox(&mut tx, current.id, "message.created", id, version).await?;
            }
            message_id = Some(id);
            message_status = Some(status.into());
        }
        let status = if actions.is_empty() {
            "committed"
        } else {
            "running"
        };
        // A slow database transaction must not commit beyond the cycle/lease deadline.
        require_lease(&mut tx, lease).await?;
        sqlx::query("UPDATE runs SET state_version=$2,status=$3,finished_at=CASE WHEN $3='committed' THEN clock_timestamp() ELSE NULL END,lease_owner=CASE WHEN $3='committed' THEN NULL ELSE lease_owner END,lease_expires_at=CASE WHEN $3='committed' THEN NULL ELSE lease_expires_at END WHERE id=$1")
            .bind(lease.run_id).bind(version).bind(status).execute(&mut *tx).await?;
        advance(&mut tx, current.id, version).await?;
        outbox(
            &mut tx,
            current.id,
            "operation.updated",
            lease.run_id,
            version,
        )
        .await?;
        let receipt = DecisionReceipt {
            decision_event_id: decision,
            memory_ids: memories,
            belief_ids: beliefs,
            autobiography_ids: autobiographies,
            appraisal_ids: affect.appraisal_ids,
            attitude_ids: affect.attitude_ids,
            relationship_revision_ids: relationship_revisions,
            development_ids: developments,
            goal_ids: goals,
            action_ids: actions,
            message_id,
            message_status,
            state_version: version,
            run_status: status.into(),
        };
        sqlx::query("INSERT INTO decision_commits(reservation_id,individual_id,run_id,lease_generation,privacy_epoch,proposal_digest,decision_event_id,receipt) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(prepared.reservation_id).bind(current.id).bind(lease.run_id).bind(lease.generation).bind(current.privacy_epoch).bind(hash).bind(decision).bind(serde_json::to_value(&receipt)?).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(receipt)
    }
}
