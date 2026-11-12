#!/usr/bin/env python3
"""Export unfilled independent review units; no model reviewer is invoked."""
import argparse
import json
import os
from pathlib import Path
if __package__:
    from .check_materials import *
    from .collect_evidence import unit_specs
    from .semantic_review import review_template
else:
    from check_materials import *
    from collect_evidence import unit_specs
    from semantic_review import review_template


def templates(materials,units,*,salt):
    reject_forbidden_capture_fields(units)
    specs=unit_specs(materials);by_id={u['unit_id']:u for u in units};private={};out=[]
    for uid,spec in specs.items():
        item=review_template(spec,by_id.get(uid),salt=salt);out.append(item)
        private[item['review_alias']]={'unit_id':uid,'kind':spec['kind'],'condition':spec.get('condition'),'candidate_response_binding_sha256':item['candidate_response_binding_sha256']}
    retrieval=materials['retrieval_holdout_v1.json']
    return ({'format':'alia-eval-independent-review-templates/1','material_manifest_sha256':MANIFEST_SHA256,
            'runtime_review_units':out,'retrieval_label_review_units':[{'query_id':q['id'],'query':q['query'],'scope':q['scope'],'at':q['at'],'include_history':q['include_history'],'query_sha256':text_sha(q['query']),
                'independently_relevant_event_ids':None,'source_scope_time_findings':None,'reviewer_id':None,'status':'not_reviewed'}for q in retrieval['queries']],
            'retrieval_label_review_corpus':[{'logical_event_id':e['logical_event_id'],'text':e['text'],'scope':e['scope'],'source_kind':e['source_kind'],'sequence':e['sequence'],'occurred_at':e['occurred_at'],'valid_from':e['valid_from'],'valid_until':e['valid_until'],'memory_validity':e['original_memory_validity']}for e in retrieval['events']],
            'status':'not_reviewed','actual_model_calls':0,'actual_db_calls':0,'formal_acceptance':None,
            'blinding_limit':'Explicit A/B/C labels and runtime unit IDs are withheld; actual adaptation/source evidence can still reveal treatment. Do not hide required evidence to improve blinding.'},
            {'format':'alia-eval-private-review-map/1','material_manifest_sha256':MANIFEST_SHA256,'map':private})


def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--input',type=Path);p.add_argument('--salt-file',type=Path,required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--private-map-output',type=Path,required=True);args=p.parse_args()
    if args.salt_file.is_symlink()or args.salt_file.stat().st_mode&0o077:raise ValueError('explicit_private_salt_required')
    salt=args.salt_file.read_text().strip();data=read_json(args.input)if args.input else {'units':[]}
    public,private=templates(load_materials(),data['units'],salt=salt)
    with os.fdopen(os.open(args.output,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600),'w')as f:json.dump(public,f,ensure_ascii=False,indent=2)
    with os.fdopen(os.open(args.private_map_output,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600),'w')as f:json.dump(private,f,ensure_ascii=False,indent=2)
    return 0

if __name__=='__main__':raise SystemExit(main())
