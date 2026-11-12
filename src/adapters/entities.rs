//! Trusted internal assertions, exact surface evidence and auditable state writes.
//! Role validation is semantic work for the caller, not proven by literal matches.
use super::{goals::unavailable, store::*};
use crate::domain::{Lifecycle, entities::*, history::ExpectedState};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;
type Tx<'a> = Transaction<'a, Postgres>;
fn surface_valid(s: &str) -> bool {
    !s.trim().is_empty()
        && s == s.trim()
        && s.chars().count() <= 80
        && !s.chars().any(char::is_control)
}
async fn quote(tx: &mut Tx<'_>, individual: Uuid, span: &SourceSpan) -> Result<String, StoreError> {
    if span.start_char >= span.end_char
        || span.end_char - span.start_char > 80
        || unavailable(tx, individual, &[span.event_id]).await?
    {
        return Err(StoreError::NotFound);
    }
    let row:Option<(String,Value)>=sqlx::query_as("SELECT kind,payload FROM events WHERE individual_id=$1 AND id=$2 AND source_kind IN ('reported_claim','direct_observation','external_document')").bind(individual).bind(span.event_id).fetch_optional(&mut **tx).await?;
    let (kind, payload) = row.ok_or(StoreError::NotFound)?;
    let text = match kind.as_str() {
        "message.received" => payload["text"].as_str().map(str::to_owned),
        "tool.result" if payload["status"] == "succeeded" => {
            payload["output"]["text"].as_str().map(str::to_owned)
        }
        "document.imported" => {
            sqlx::query_scalar("SELECT body FROM documents WHERE individual_id=$1 AND event_id=$2")
                .bind(individual)
                .bind(span.event_id)
                .fetch_optional(&mut **tx)
                .await?
        }
        _ => None,
    }
    .ok_or(StoreError::NotFound)?;
    let chars: Vec<char> = text.chars().collect();
    let value: String = chars
        .get(span.start_char..span.end_char)
        .ok_or(StoreError::Invalid("entity source span"))?
        .iter()
        .collect();
    if !surface_valid(&value) {
        return Err(StoreError::Invalid("entity surface"));
    }
    Ok(value)
}
async fn verify_state(
    tx: &mut Tx<'_>,
    expected: ExpectedState,
) -> Result<crate::domain::history::IndividualState, StoreError> {
    let current = state(tx).await?.ok_or(StoreError::NotInitialized)?;
    if current.state_version != expected.version
        || current.privacy_epoch != expected.privacy_epoch
        || unavailable(tx, current.id, &[current.id]).await?
    {
        return Err(StoreError::StaleState);
    }
    if current.lifecycle != Lifecycle::Active {
        return Err(StoreError::Inactive);
    }
    Ok(current)
}
impl Store {
    pub async fn register_person(
        &self,
        key: &str,
        expected: ExpectedState,
        external_key: &str,
        name: &str,
        source: SourceSpan,
    ) -> Result<EntityReceipt, StoreError> {
        if !surface_valid(name)
            || external_key.is_empty()
            || external_key.len() > 120
            || external_key.chars().any(char::is_control)
        {
            return Err(StoreError::Invalid("person input"));
        }
        let hash = digest(
            &json!({"expected":expected,"external_key":external_key,"name":name,"source":source}),
        )?;
        let mut tx = self.write().await?;
        if let Some(r) =
            replay::<EntityReceipt>(&mut tx, "owner", "/internal/persons", key, &hash).await?
        {
            let individual = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?.id;
            if unavailable(&mut tx, individual, &[individual, r.entity_id, r.event_id]).await? {
                return Err(StoreError::NotFound);
            }
            return Ok(r);
        }
        let current = verify_state(&mut tx, expected).await?;
        if quote(&mut tx, current.id, &source).await? != name {
            return Err(StoreError::Invalid("person source mismatch"));
        }
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM person_entities WHERE individual_id=$1 AND external_key=$2)").bind(current.id).bind(external_key).fetch_one(&mut *tx).await?;
        if exists {
            return Err(StoreError::IdempotencyConflict);
        }
        let entity = Uuid::new_v4();
        let event = Uuid::new_v4();
        let version = current.state_version + 1;
        object(&mut tx, current.id, entity, "entity").await?;
        insert_event(
            &mut tx,
            current.id,
            event,
            "person.registered",
            "reported_claim",
            "owner",
            json!({"entity_id":entity,"name":name,"source":source}),
            entity,
            Some(key),
        )
        .await?;
        sqlx::query(
            "INSERT INTO person_entities(individual_id,id,external_key,label) VALUES($1,$2,$3,$4)",
        )
        .bind(current.id)
        .bind(entity)
        .bind(external_key)
        .bind(name)
        .execute(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO person_aliases VALUES($1,$2,$3,$4,$5)")
            .bind(current.id)
            .bind(entity)
            .bind(name)
            .bind(source.event_id)
            .bind(event)
            .execute(&mut *tx)
            .await?;
        edge(&mut tx, current.id, source.event_id, event).await?;
        edge(&mut tx, current.id, event, entity).await?;
        advance(&mut tx, current.id, version).await?;
        outbox(&mut tx, current.id, "state.changed", current.id, version).await?;
        let receipt = EntityReceipt {
            entity_id: entity,
            event_id: event,
            version,
        };
        save_receipt(&mut tx, "owner", "/internal/persons", key, &hash, &receipt).await?;
        tx.commit().await?;
        Ok(receipt)
    }
    pub async fn add_person_alias(
        &self,
        key: &str,
        expected: ExpectedState,
        entity_id: Uuid,
        source: SourceSpan,
    ) -> Result<EntityReceipt, StoreError> {
        let hash = digest(&json!({"expected":expected,"entity_id":entity_id,"source":source}))?;
        let mut tx = self.write().await?;
        if let Some(r) =
            replay::<EntityReceipt>(&mut tx, "owner", "/internal/person-aliases", key, &hash)
                .await?
        {
            let id = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?.id;
            if unavailable(&mut tx, id, &[id, r.entity_id, r.event_id]).await? {
                return Err(StoreError::NotFound);
            }
            return Ok(r);
        }
        let current = verify_state(&mut tx, expected).await?;
        if unavailable(&mut tx, current.id, &[entity_id]).await? {
            return Err(StoreError::NotFound);
        }
        let surface = quote(&mut tx, current.id, &source).await?;
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM person_entities WHERE individual_id=$1 AND id=$2)",
        )
        .bind(current.id)
        .bind(entity_id)
        .fetch_one(&mut *tx)
        .await?;
        if !exists {
            return Err(StoreError::NotFound);
        }
        let event = Uuid::new_v4();
        let version = current.state_version + 1;
        insert_event(
            &mut tx,
            current.id,
            event,
            "person.alias_asserted",
            "reported_claim",
            "owner",
            json!({"entity_id":entity_id,"surface":surface,"source":source}),
            entity_id,
            Some(key),
        )
        .await?;
        let result =
            sqlx::query("INSERT INTO person_aliases VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING")
                .bind(current.id)
                .bind(entity_id)
                .bind(surface)
                .bind(source.event_id)
                .bind(event)
                .execute(&mut *tx)
                .await?;
        if result.rows_affected() != 1 {
            return Err(StoreError::IdempotencyConflict);
        }
        edge(&mut tx, current.id, source.event_id, event).await?;
        edge(&mut tx, current.id, entity_id, event).await?;
        advance(&mut tx, current.id, version).await?;
        outbox(&mut tx, current.id, "state.changed", current.id, version).await?;
        let receipt = EntityReceipt {
            entity_id,
            event_id: event,
            version,
        };
        save_receipt(
            &mut tx,
            "owner",
            "/internal/person-aliases",
            key,
            &hash,
            &receipt,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }
    pub async fn resolve_person(&self, surface: &str) -> Result<PersonResolution, StoreError> {
        if !surface_valid(surface) {
            return Err(StoreError::Invalid("person surface"));
        }
        let mut tx = self.write().await?;
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        if unavailable(&mut tx, current.id, &[current.id]).await? {
            return Err(StoreError::NotFound);
        }
        let candidates:Vec<(Uuid,Uuid,Uuid)>=sqlx::query_as("SELECT entity_id,source_event_id,assertion_event_id FROM person_aliases WHERE individual_id=$1 AND surface=$2 ORDER BY entity_id").bind(current.id).bind(surface).fetch_all(&mut *tx).await?;
        let mut ids = vec![];
        for (id, source, event) in candidates {
            if !unavailable(&mut tx, current.id, &[id, source, event]).await? {
                ids.push(id);
            }
        }
        tx.commit().await?;
        Ok(match ids.as_slice() {
            [] => PersonResolution::Unknown,
            [id] => PersonResolution::Unique(*id),
            _ => PersonResolution::Ambiguous,
        })
    }
    pub async fn assert_memory_person(
        &self,
        key: &str,
        expected: ExpectedState,
        assertion: MemoryPersonAssertion,
    ) -> Result<EntityReceipt, StoreError> {
        let hash = digest(&json!({"expected":expected,"assertion":assertion}))?;
        let mut tx = self.write().await?;
        if let Some(r) =
            replay::<EntityReceipt>(&mut tx, "owner", "/internal/memory-persons", key, &hash)
                .await?
        {
            let individual = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?.id;
            if unavailable(
                &mut tx,
                individual,
                &[
                    individual,
                    r.event_id,
                    assertion.entity_id,
                    assertion.memory_id,
                ],
            )
            .await?
            {
                return Err(StoreError::NotFound);
            }
            return Ok(r);
        }
        let current = verify_state(&mut tx, expected).await?;
        if unavailable(
            &mut tx,
            current.id,
            &[assertion.entity_id, assertion.memory_id],
        )
        .await?
        {
            return Err(StoreError::NotFound);
        }
        let text = quote(&mut tx, current.id, &assertion.source).await?;
        let row:Option<(String,i64)>=sqlx::query_as("SELECT content,version FROM memories WHERE individual_id=$1 AND id=$2 AND EXISTS(SELECT 1 FROM memory_sources WHERE individual_id=$1 AND memory_id=$2 AND event_id=$3)").bind(current.id).bind(assertion.memory_id).bind(assertion.source.event_id).fetch_optional(&mut *tx).await?;
        let (content, version) = row.ok_or(StoreError::NotFound)?;
        if version != assertion.memory_version {
            return Err(StoreError::StaleState);
        }
        let chars: Vec<char> = content.chars().collect();
        let end = assertion
            .memory_start_char
            .checked_add(text.chars().count())
            .ok_or(StoreError::Invalid("person memory span"))?;
        if chars
            .get(assertion.memory_start_char..end)
            .is_none_or(|s| s.iter().copied().ne(text.chars()))
        {
            return Err(StoreError::Invalid("person memory quote mismatch"));
        }
        let alias:Option<(Uuid,Uuid)>=sqlx::query_as("SELECT source_event_id,assertion_event_id FROM person_aliases WHERE individual_id=$1 AND entity_id=$2 AND surface=$3").bind(current.id).bind(assertion.entity_id).bind(&text).fetch_optional(&mut *tx).await?;
        let (alias_source, alias_event) = alias.ok_or(StoreError::NotFound)?;
        if unavailable(&mut tx, current.id, &[alias_source, alias_event]).await? {
            return Err(StoreError::NotFound);
        }
        let event = Uuid::new_v4();
        let version = current.state_version + 1;
        insert_event(
            &mut tx,
            current.id,
            event,
            "memory.person_asserted",
            "reported_claim",
            "owner",
            json!({"assertion":assertion}),
            assertion.memory_id,
            Some(key),
        )
        .await?;
        sqlx::query("INSERT INTO memory_person_links(individual_id,memory_id,memory_version,entity_id,role,source_event_id,assertion_event_id,content_digest) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(individual_id,memory_id,entity_id) DO UPDATE SET role=EXCLUDED.role,memory_version=EXCLUDED.memory_version,source_event_id=EXCLUDED.source_event_id,assertion_event_id=EXCLUDED.assertion_event_id,content_digest=EXCLUDED.content_digest").bind(current.id).bind(assertion.memory_id).bind(assertion.memory_version).bind(assertion.entity_id).bind(assertion.role.as_str()).bind(assertion.source.event_id).bind(event).bind(format!("{:x}",Sha256::digest(content.as_bytes()))).execute(&mut *tx).await?;
        for source in [assertion.entity_id, assertion.source.event_id, alias_event] {
            edge(&mut tx, current.id, source, event).await?;
        }
        edge(&mut tx, current.id, event, assertion.memory_id).await?;
        advance(&mut tx, current.id, version).await?;
        outbox(
            &mut tx,
            current.id,
            "memory.changed",
            assertion.memory_id,
            version,
        )
        .await?;
        let receipt = EntityReceipt {
            entity_id: assertion.entity_id,
            event_id: event,
            version,
        };
        save_receipt(
            &mut tx,
            "owner",
            "/internal/memory-persons",
            key,
            &hash,
            &receipt,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }
}

pub(super) async fn catalog(
    connection: &mut sqlx::PgConnection,
    individual: Uuid,
) -> Result<Vec<(PersonRef, Vec<Uuid>, bool)>, StoreError> {
    type Row = (Uuid, String, String, Uuid, Uuid);
    let rows:Vec<Row>=sqlx::query_as(r#"WITH RECURSIVE denied(id) AS (
 SELECT id FROM objects WHERE individual_id=$1 AND blocked UNION
 SELECT d.target_id FROM derivation_edges d JOIN denied ON denied.id=d.source_id WHERE d.individual_id=$1
 ), persons AS (SELECT id,label FROM person_entities WHERE individual_id=$1 AND NOT EXISTS(SELECT 1 FROM denied WHERE denied.id=person_entities.id) ORDER BY id LIMIT 21)
 SELECT p.id,p.label,a.surface,a.source_event_id,a.assertion_event_id FROM persons p CROSS JOIN LATERAL (
 SELECT surface,source_event_id,assertion_event_id FROM person_aliases a WHERE a.individual_id=$1 AND a.entity_id=p.id
 AND NOT EXISTS(SELECT 1 FROM denied WHERE denied.id IN (a.source_event_id,a.assertion_event_id)) ORDER BY surface LIMIT 9
 ) a ORDER BY p.id,a.surface"#).bind(individual).fetch_all(connection).await?;
    let mut result: Vec<(PersonRef, Vec<Uuid>, bool)> = vec![];
    for (id, label, surface, source, event) in rows {
        if result.last().is_none_or(|r| r.0.id != id) {
            result.push((
                PersonRef {
                    id,
                    label,
                    aliases: vec![],
                },
                vec![id],
                false,
            ));
        }
        let last = result.last_mut().unwrap();
        if last.0.aliases.len() == 8 {
            last.2 = true;
            continue;
        }
        last.0.aliases.push(surface);
        last.1.extend([source, event]);
    }
    Ok(result)
}

pub(super) async fn apply_model_proposals(
    tx: &mut Tx<'_>,
    individual: Uuid,
    decision: Uuid,
    version: i64,
    proposals: &[EntityProposal],
    created: &[Uuid],
    context: &crate::domain::cognition::ModelContext,
) -> Result<(), StoreError> {
    let mut people = vec![];
    let mut pairs = std::collections::HashSet::new();
    for (index, p) in proposals.iter().enumerate() {
        let surface = quote(tx, individual, &p.source).await?;
        let entity = match &p.person {
            PersonTarget::Existing { id } => *id,
            PersonTarget::Prior { index } => *people
                .get(*index)
                .ok_or(StoreError::Invalid("person prior reference"))?,
            PersonTarget::New { name } => {
                if name != &surface {
                    return Err(StoreError::Invalid("person source mismatch"));
                }
                // Tombstoned names remain reserved: do not silently recreate deleted people.
                let known:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM person_aliases WHERE individual_id=$1 AND surface=$2)").bind(individual).bind(name).fetch_one(&mut **tx).await?;
                if known {
                    return Err(crate::domain::cognition::ProposalError::Entity.into());
                }
                let id = Uuid::new_v4();
                let event = Uuid::new_v4();
                object(tx, individual, id, "entity").await?;
                insert_event(
                    tx,
                    individual,
                    event,
                    "person.inferred",
                    "inference",
                    "alia",
                    json!({"entity_id":id,"name":name,"source":p.source,"decision_id":decision}),
                    decision,
                    None,
                )
                .await?;
                sqlx::query("INSERT INTO person_entities VALUES($1,$2,$3,$4)")
                    .bind(individual)
                    .bind(id)
                    .bind(format!("model:{decision}:{index}"))
                    .bind(name)
                    .execute(&mut **tx)
                    .await?;
                sqlx::query("INSERT INTO person_aliases VALUES($1,$2,$3,$4,$5)")
                    .bind(individual)
                    .bind(id)
                    .bind(name)
                    .bind(p.source.event_id)
                    .bind(event)
                    .execute(&mut **tx)
                    .await?;
                edge(tx, individual, p.source.event_id, event).await?;
                edge(tx, individual, decision, event).await?;
                edge(tx, individual, event, id).await?;
                id
            }
        };
        people.push(entity);
        let memory = match p.memory {
            MemoryTarget::Existing { id } => id,
            MemoryTarget::Created { index } => *created
                .get(index)
                .ok_or(StoreError::Invalid("created memory reference"))?,
        };
        if unavailable(tx, individual, &[entity, memory]).await? {
            return Err(StoreError::StaleState);
        }
        if !pairs.insert((memory, entity)) {
            return Err(crate::domain::cognition::ProposalError::Entity.into());
        }
        let (content,memory_version):(String,i64)=sqlx::query_as("SELECT content,version FROM memories WHERE individual_id=$1 AND id=$2 AND EXISTS(SELECT 1 FROM memory_sources WHERE individual_id=$1 AND memory_id=$2 AND event_id=$3)").bind(individual).bind(memory).bind(p.source.event_id).fetch_optional(&mut **tx).await?.ok_or(StoreError::NotFound)?;
        if let MemoryTarget::Existing { id } = p.memory
            && context
                .memories
                .iter()
                .find(|m| m.id == id)
                .is_none_or(|m| m.version != memory_version)
        {
            return Err(StoreError::StaleState);
        }
        let memory_start_char =
            crate::domain::entities::memory_name_offset(&content, &surface, p.memory_start_char)
                .map_err(crate::domain::cognition::ProposalError::EntityProposal)?;
        let (alias_source,alias_event):(Uuid,Uuid)=sqlx::query_as("SELECT source_event_id,assertion_event_id FROM person_aliases WHERE individual_id=$1 AND entity_id=$2 AND surface=$3").bind(individual).bind(entity).bind(surface).fetch_optional(&mut **tx).await?.ok_or(StoreError::NotFound)?;
        if unavailable(tx, individual, &[alias_source, alias_event]).await? {
            return Err(StoreError::StaleState);
        }
        let event = Uuid::new_v4();
        insert_event(tx,individual,event,"memory.person_inferred","inference","alia",json!({"entity_id":entity,"memory_id":memory,"role":p.role,"source":p.source,"memory_start_char":memory_start_char,"decision_id":decision}),decision,None).await?;
        sqlx::query("INSERT INTO memory_person_links VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(individual_id,memory_id,entity_id) DO UPDATE SET memory_version=EXCLUDED.memory_version,content_digest=EXCLUDED.content_digest,role=EXCLUDED.role,source_event_id=EXCLUDED.source_event_id,assertion_event_id=EXCLUDED.assertion_event_id").bind(individual).bind(memory).bind(memory_version).bind(format!("{:x}",Sha256::digest(content.as_bytes()))).bind(entity).bind(p.role.as_str()).bind(p.source.event_id).bind(event).execute(&mut **tx).await?;
        for source in [decision, entity, alias_event, p.source.event_id] {
            edge(tx, individual, source, event).await?;
        }
        edge(tx, individual, event, memory).await?;
        outbox(tx, individual, "memory.changed", memory, version).await?;
    }
    Ok(())
}
