#!/usr/bin/env python3
"""One natural user turn at a time against a Root-released isolated service.

No initialization, provider keys, runtime configuration changes or retry loops.
The tester reads each actual result and chooses the next turn. The private Root
session supplies the program-enforced project accounting observer, not a budget
invented by this driver. Semantic review is separate from engineering capture.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import time
from urllib.parse import urlsplit
from urllib.request import Request, urlopen
import uuid

sys.path.insert(0, str(Path(__file__).resolve().parent / 'qa'))
from common import clean_env, read_json, digest, resource_locks


def require(ok, code):
    if not ok:
        raise ValueError(code)


def now():
    return datetime.now(timezone.utc).isoformat()


def uid(value):
    try:
        return isinstance(value, str) and str(uuid.UUID(value)) == value and uuid.UUID(value).int != 0
    except ValueError:
        return False


def sha(value):
    return isinstance(value, str) and len(value) == 64 and all(c in '0123456789abcdef' for c in value)


def binding_hash(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',',':')).encode()).hexdigest()


def adopt_model_boundary(report, binding, session):
    if report['binding'] == binding:
        return
    old = report['binding']
    require(all(old[k] == binding[k] for k in ('individual_id','campaign_ledger_id')), 'campaign_identity_change_refused')
    boundary = session.get('model_boundary', {})
    require(boundary.get('previous_binding_sha256') == binding_hash(old) and
            boundary.get('new_binding_sha256') == binding_hash(binding) and
            boundary.get('released_by') in ('Root','coordination-window') and isinstance(boundary.get('reason'),str) and
            bool(boundary['reason'].strip()), 'explicit_model_boundary_required')
    # Root's private release is a trust boundary, not a cryptographic signature.
    report.setdefault('model_boundaries',[]).append({'old_binding':old,'new_binding':binding,
        'root_release':boundary,'recorded_at':now(),'last_actual_usage':report['turns'][-1].get('accounting_after') if report['turns'] else None})
    report['binding'] = binding
    report['model_label'] = session.get('model_label','model identity unobserved')
    report['display_version'] = session.get('display_version','fixed release')


def save(path, value):
    temporary = path.with_name(path.name + '.partial')
    with temporary.open('w') as stream:
        json.dump(value, stream, ensure_ascii=False, indent=2, allow_nan=False)
        stream.flush(); os.fsync(stream.fileno())
    temporary.replace(path)
    if value.get('format') == 'alia-natural-transcript/1' and value.get('task_id') == 'VISIBLE-NATURAL-01':
        public_path = Path(__file__).resolve().parents[1] / 'evals/results/visible-demo-01/natural-live.json'
        public = {'status': value['turns'][-1]['status'] if value['turns'] else 'pending',
            'version':value.get('display_version','N5R3'),
            'model':value.get('model_label','glm-5.3-flash（配置；返回模型标识未单独观测）'),
            'binding':value['binding'], 'updated_at':value.get('updated_at',now()),
            'historical_campaign_cost':{'N2_charged_tokens':84609,'all_history_total':None,'money_cost':None},
            'formal_acceptance':None, 'turns':[
                {'id':t['id'],'layer':t['layer'],'user':t['user'],
                 'alia':'\n\n'.join(r['text'] for r in t['replies']) if t['replies'] else None,
                 'status':t['status'],'usage':t.get('actual_usage'),'run_id':t['operation_id'],
                 'model_profile':t.get('model_profile'),
                 'error_code':t.get('error_code'),
                 'runtime_error_code':t.get('terminal_operation',{}).get('error_code'),
                 'started_at':t['started_at']}
                for t in value['turns']]}
        public_tmp = public_path.with_suffix('.partial')
        with public_tmp.open('w') as stream:
            json.dump(public,stream,ensure_ascii=False,indent=2,allow_nan=False)
            stream.flush();os.fsync(stream.fileno())
        public_tmp.replace(public_path)


def usage_summary(rows, run_id=None):
    require(isinstance(rows, list), 'usage_missing')
    seen = {}
    for row in rows:
        require(isinstance(row, dict) and uid(row.get('id')) and uid(row.get('run_id')), 'usage_identity')
        require(row.get('status') in ('reserved', 'dispatched', 'settled') and
                type(row.get('usage_unknown')) is bool, 'usage_state')
        for key in ('actual_input', 'actual_output', 'charged_tokens'):
            require(row.get(key) is None or type(row[key]) is int and row[key] >= 0, 'usage_value')
        require(row['id'] not in seen, 'duplicate_reservation')
        seen[row['id']] = row
    selected = [r for r in seen.values() if run_id is None or r['run_id'] == run_id]
    known = all(r['status'] == 'settled' and not r['usage_unknown'] and
                all(type(r[k]) is int for k in ('actual_input','actual_output','charged_tokens')) for r in selected)
    # No measured row for a dispatched run is unknown, never zero.
    complete = known and (run_id is None or bool(selected))
    return {'reservation_ids': [r['id'] for r in selected], 'calls': len(selected),
        'actual_input': sum(r['actual_input'] for r in selected) if complete else None,
        'actual_output': sum(r['actual_output'] for r in selected) if complete else None,
        'charged_tokens': sum(r['charged_tokens'] for r in selected) if complete else None,
        'observed_ledger_charges': sum(r['charged_tokens'] for r in selected if type(r['charged_tokens']) is int),
        'usage_complete': complete, 'monetary_cost': None}


def billing_settled(rows):
    """Actual usage can remain unknown after full conservative settlement."""
    return all(r.get('status') == 'settled' and type(r.get('charged_tokens')) is int and
        (type(r.get('reserved_tokens')) is int and r['charged_tokens'] == r['reserved_tokens']
         if r.get('usage_unknown') else
         type(r.get('actual_input')) is int and type(r.get('actual_output')) is int and
         r['charged_tokens'] == r['actual_input'] + r['actual_output']) for r in rows)


class Transport:
    def __init__(self, session):
        self.session = session

    def api(self, method, path, body=None, key=None):
        headers = {'Authorization': 'Bearer ' + self.session['token']}
        if key:
            headers['Idempotency-Key'] = key
        if body is not None:
            headers['Content-Type'] = 'application/json'
        request = Request(self.session['base_url'] + path, method=method,
            headers=headers, data=None if body is None else json.dumps(body, ensure_ascii=False).encode())
        with urlopen(request, timeout=30) as response:
            self.last_http_status = response.status
            return json.load(response)

    def accounting(self):
        result = subprocess.run(self.session['observer_command'], env=clean_env(),
            capture_output=True, text=True, timeout=30, check=False)
        require(result.returncode == 0, 'observer_failed')
        return json.loads(result.stdout)  # Root observer emits allowlisted metadata only.


def validate_accounting(session, value, before_dispatch):
    require(isinstance(value, dict) and value.get('release_id') == session['release_id'] and
            value.get('individual_id') == session['individual_id'] and
            value.get('campaign_ledger_id') == session['campaign_ledger_id'], 'accounting_binding')
    require(value.get('inventory_complete') is True, 'incomplete_campaign_inventory')
    summary = usage_summary(value.get('usage'))
    require(type(value.get('all_campaign_charged_tokens')) is int and
            value['all_campaign_charged_tokens'] >= summary['observed_ledger_charges'], 'campaign_charge_missing')
    require(type(value.get('budget_frozen')) is bool and type(value.get('dispatch_allowed')) is bool,
            'budget_state_missing')
    if before_dispatch:
        require(not value['budget_frozen'] and value['dispatch_allowed'] and billing_settled(value['usage']),
                'budget_wait_or_unsettled')
    return {'campaign_ledger_id': value['campaign_ledger_id'],
        'all_campaign_charged_tokens': value['all_campaign_charged_tokens'],
        'budget_frozen': value['budget_frozen'], 'dispatch_allowed': value['dispatch_allowed'],
        'usage': summary}


def run_turn(session, report, path, case, text, transport, *, acknowledgement=None, timeout=600, sleep=time.sleep):
    # Explicitly resuming a preflight that provably never sent is permitted;
    # retain the old attempt. A dispatched/unknown turn is never sent again.
    prior_attempts = []
    if report['turns'] and report['turns'][-1]['id'] == case['id'] and report['turns'][-1]['status'] == 'not_dispatched':
        old = report['turns'][-1]
        require(old['operation_id'] is None, 'previous_outcome_unknown')
        prior_attempts = old.get('preflight_attempts', []) + [{k:v for k,v in old.items() if k != 'preflight_attempts'}]
        report['turns'].pop()
    require(case['id'] not in {t['id'] for t in report['turns']}, 'turn_already_recorded_no_retry')
    require(not report['turns'] or report['turns'][-1]['status'] in ('committed','failed','cancelled','stale'),
            'previous_outcome_unknown')
    if report['turns']:
        previous = report['turns'][-1]
        require(isinstance(acknowledgement, dict) and acknowledgement.get('turn_id') == previous['id'] and
                isinstance(acknowledgement.get('observation'), str) and acknowledgement['observation'].strip() and
                isinstance(acknowledgement.get('next_reason'), str) and acknowledgement['next_reason'].strip(),
                'actual_result_review_required')
        quote = acknowledgement.get('actual_reply_quote')
        replies = '\n'.join(r['text'] for r in previous.get('replies', []))
        require(isinstance(quote, str) and (bool(quote) and quote in replies or not replies and quote == ''),
                'acknowledgement_quote_not_actual')
        previous['tester_observation'] = acknowledgement
    require(isinstance(text, str) and text.strip() and '\0' not in text and len(text.encode()) <= 16*1024,
            'invalid_user_text')
    row = {'id':case['id'], 'layer':case['layer'], 'kind':case['kind'], 'user':text,
        'started_at':now(), 'status':'preflight', 'version':report['binding'], 'replies':[],
        'operation_id':None, 'idempotency_key':str(uuid.uuid4()), 'actual_usage':None,
        'semantic_verdict':None, 'error_code':None,'model_profile':session.get('model_profile')}
    row['preflight_attempts'] = prior_attempts
    report['turns'].append(row); save(path,report)
    try:
        state = transport.api('GET','/state')
        require(state['individual_id'] == session['individual_id'], 'individual_changed')
        diagnostics = transport.api('GET','/diagnostics')
        row['service_started_at'] = diagnostics.get('service_started_at')
        if case.get('checkpoint_before') == 'restart_same_individual':
            require(len(report['turns']) > 1 and isinstance(row['service_started_at'],str) and
                    row['service_started_at'] != report['turns'][-2].get('service_started_at'), 'restart_not_observed')
        before = transport.accounting()
        row['accounting_before'] = validate_accounting(session,before,True)
        row['state_version_before'] = state.get('version')
        # No rubric, hidden facts or model reply template crosses this boundary.
        row['status'] = 'sending_outcome_unknown'; save(path,report)
        receipt = transport.api('POST','/messages',{'text':text,'conversation_id':session['conversation_id']},row['idempotency_key'])
        row['submit_http_status'] = getattr(transport,'last_http_status',None)
        require(uid(receipt.get('operation_id')) and uid(receipt.get('event_id')), 'invalid_submission_receipt')
        row['operation_id'] = receipt['operation_id']; row['input_event_id'] = receipt['event_id']
        row['status'] = 'running'; save(path,report)
        deadline = time.monotonic() + timeout
        while True:
            operation = transport.api('GET','/operations/'+row['operation_id'])
            status = operation.get('status')
            if status in ('committed','failed','cancelled','stale'):
                row['status'] = status
                row['terminal_operation'] = {k:operation.get(k) for k in (
                    'operation_id','status','mode','version','model_calls','tool_calls','error_code')}
                break
            require(time.monotonic() < deadline, 'run_outcome_unknown_timeout')
            sleep(1)
        messages = transport.api('GET','/messages?order=desc&limit=100')
        row['replies'] = [{k:r[k] for k in ('id','text','occurred_at','operation_id')}
            for r in reversed(messages['items']) if r.get('role') == 'assistant' and
            r.get('operation_id') == row['operation_id']]
        if row['status'] == 'committed' and not row['replies']:
            row['error_code'] = 'committed_without_reply'
        try:
            trace = transport.api('GET','/runs/'+row['operation_id'])
            require(trace.get('individual_id') == session['individual_id'] and
                    trace.get('run',{}).get('id') == row['operation_id'] and
                    trace.get('payload_policy') == 'metadata_only', 'trace_binding')
            row['execution_trace'] = {k:trace[k] for k in ('individual_id','run','spans','spans_more','payload_policy')}
            row['input_source'] = transport.api('GET','/source-events/'+row['input_event_id'])
            require(row['input_source'].get('individual_id') == session['individual_id'] and
                    row['input_source'].get('event',{}).get('id') == row['input_event_id'], 'input_source_binding')
        except Exception:
            row['source_observation'] = 'unverified; no raw trace or hidden context substituted'
        after = transport.accounting()
        row['accounting_after'] = validate_accounting(session,after,False)
        row['actual_usage'] = usage_summary(after['usage'],row['operation_id'])
    except Exception as error:
        # Never save an exception body, HTTP body, credentials or raw supplier output.
        row['error_code'] = str(error) if isinstance(error,ValueError) and str(error) in {
            'individual_changed','budget_wait_or_unsettled','run_outcome_unknown_timeout',
            'accounting_binding','incomplete_campaign_inventory','campaign_charge_missing',
            'usage_missing','usage_identity','usage_state','usage_value','duplicate_reservation',
            'observer_failed','invalid_submission_receipt','budget_state_missing','restart_not_observed'} else type(error).__name__
        if row['status'] == 'preflight':
            row['status'] = 'not_dispatched'
        elif row['status'] not in ('committed','failed','cancelled','stale'):
            row['status'] = 'outcome_unknown'
        # Recover accounting without dispatching another request.
        try:
            after = transport.accounting(); row['accounting_after'] = validate_accounting(session,after,False)
            row['actual_usage'] = usage_summary(after['usage'],row['operation_id']) if row['operation_id'] else None
        except Exception:
            row['accounting_recovery'] = 'unknown'
    row['finished_at']=now(); report['updated_at']=now(); save(path,report)
    return row


def session_from_private(path, plan_path):
    require(path.is_file() and not path.is_symlink() and path.stat().st_uid == os.getuid() and
            stat.S_IMODE(path.stat().st_mode) == 0o600,'private_session_required')
    s=read_json(path)
    require(not s.get('dispatch_suspended_reason'), 'dispatch_suspended')
    require(s.get('format')=='alia-natural-session/1' and s.get('task_id')=='VISIBLE-NATURAL-01', 'unreleased_session')
    require(s.get('plan_sha256')==digest(plan_path) and s.get('driver_sha256')==digest(__file__), 'driver_plan_binding')
    require(uid(s.get('individual_id')) and uid(s.get('conversation_id')), 'session_identity')
    u=urlsplit(s['base_url'])
    require(u.scheme=='http' and u.hostname=='127.0.0.1' and u.port and u.path=='/api/v1' and
            not u.username and not u.password and not u.query and not u.fragment,'isolated_loopback_required')
    require(isinstance(s.get('token'),str) and bool(s['token']), 'token_missing')
    require(isinstance(s.get('observer_command'),list) and s['observer_command'] and
            all(isinstance(a,str) and a for a in s['observer_command']), 'observer_command_missing')
    require(isinstance(s.get('observer_files'),dict) and bool(s['observer_files']), 'observer_source_missing')
    for name,h in s['observer_files'].items(): require(sha(h) and digest(name)==h,'observer_source_changed')
    require(sha(s.get('source_manifest_sha256')) and digest(s['source_manifest'])==s['source_manifest_sha256'], 'source_manifest_changed')
    require(sha(s.get('artifact_manifest_sha256')) and digest(s['artifact_manifest'])==s['artifact_manifest_sha256'], 'artifact_manifest_changed')
    require(digest(s['service_binary'])==s['service_binary_sha256'],'service_binary_changed')
    require(isinstance(s.get('resource_locks'),list) and bool(s['resource_locks']), 'resource_binding_missing')
    return s


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--session',type=Path,required=True);parser.add_argument('--plan',type=Path,required=True)
    parser.add_argument('--report',type=Path,required=True);parser.add_argument('--case',required=True)
    parser.add_argument('--ack',type=Path);parser.add_argument('--followup',type=Path)
    args=parser.parse_args()
    session=session_from_private(args.session,args.plan);plan=read_json(args.plan)
    case=next(c for c in plan['core_turns'] if c['id']==args.case)
    text=case['user_text']
    if args.followup:
        follow=read_json(args.followup);text=follow['user_text']
        case={'id':follow['id'],'layer':case['layer'],'kind':'adaptive_followup'}
    with resource_locks(('model','shared'),*[tuple(v) for v in session['resource_locks']]):
        binding={k:session[k] for k in ('release_id','individual_id','campaign_ledger_id','source_manifest_sha256','artifact_manifest_sha256','service_binary_sha256')}
        if args.report.exists():
            report=read_json(args.report);require(report['plan_sha256']==digest(args.plan),'report_binding_changed')
            adopt_model_boundary(report,binding,session)
        else:
            report={'format':'alia-natural-transcript/1','task_id':plan['task_id'],'plan_sha256':digest(args.plan),
                'binding':binding,'turns':[],'formal_acceptance':None,'updated_at':now()}
            # Reserve the path before any possible external mutation.
            with args.report.open('x') as stream: json.dump(report,stream)
        row=run_turn(session,report,args.report,case,text,Transport(session),
            acknowledgement=read_json(args.ack) if args.ack else None)
    print(json.dumps(row,ensure_ascii=False))
    return 0 if row['status']=='committed' and row['error_code'] is None else 1


if __name__=='__main__':
    raise SystemExit(main())
