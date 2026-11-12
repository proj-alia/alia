-- Keep the latest current role per memory/person. Older assertions remain events.
DELETE FROM memory_person_links old
USING memory_person_links newer, events old_event, events new_event
WHERE old.individual_id=newer.individual_id AND old.memory_id=newer.memory_id
 AND old.entity_id=newer.entity_id AND old.role<>newer.role
 AND (old_event.individual_id,old_event.id)=(old.individual_id,old.assertion_event_id)
 AND (new_event.individual_id,new_event.id)=(newer.individual_id,newer.assertion_event_id)
 AND old_event.sequence<new_event.sequence;
ALTER TABLE memory_person_links DROP CONSTRAINT memory_person_links_pkey;
ALTER TABLE memory_person_links ADD PRIMARY KEY(individual_id,memory_id,entity_id);
