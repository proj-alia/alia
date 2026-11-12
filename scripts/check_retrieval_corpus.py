#!/usr/bin/env python3
"""Validate frozen labels only. Does not run retrieval or estimate recall."""
import hashlib
import json
from datetime import datetime
from pathlib import Path

ROOT=Path(__file__).resolve().parents[1]/'evals/retrieval'
path=ROOT/'zh-v1.json'
expected=(ROOT/'zh-v1.sha256').read_text().split()[0]
assert hashlib.sha256(path.read_bytes()).hexdigest()==expected, 'Frozen corpus changed; use a new version'
data=json.loads(path.read_text())
memories={m['id']:m for m in data['memories']}
events={e['id']:e for e in data['events']}
assert len(memories)==len(data['memories'])==80
assert len(events)==len(data['events'])==80
assert len(data['queries'])==100
assert len({q['id'] for q in data['queries']})==100
assert len({q['text'] for q in data['queries']})==100
categories={}
for memory in memories.values():
    assert memory['source_event_ids'] and set(memory['source_event_ids'])<=events.keys()
    for source in memory['source_event_ids']:
        assert events[source]['text']==memory['content']
for query in data['queries']:
    categories[query['category']]=categories.get(query['category'],0)+1
    relevant=set(query['relevant_ids']); forbidden=set(query['forbidden_ids'])
    supplemental=set(query['supplemental_ids'])
    assert not supplemental & (relevant|forbidden) and supplemental <= memories.keys()
    assert relevant and not relevant&forbidden
    assert relevant|forbidden <= memories.keys()
    at=datetime.fromisoformat(query['at'])
    for key in relevant:
        m=memories[key]
        assert m['subject']==query['subject']
        assert m['scope'] in ('general',query['scope'])
        assert datetime.fromisoformat(m['valid_from'])<=at
        assert m['valid_until'] is None or at<datetime.fromisoformat(m['valid_until'])
        assert m['validity']=='active' or query['include_history']
assert set(categories.values())=={20} and len(categories)==5
print('Corpus PASS: 80 synthetic memories/events, 100 labeled queries, five categories; SHA256='+expected)
print('This checker does not run retrieval; measured baselines are separate reports, not label validation')
