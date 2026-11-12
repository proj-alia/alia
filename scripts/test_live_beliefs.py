#!/usr/bin/env python3
"""Explicit frozen v5 belief trial parent. Default is file-only preparation.

Actual execution is deferred until complete M5 engineering qualification and a
separate root-released named job. B03 remains preparation only; no implicit
credentials, .env, Compose, build, clock adjustment, or second trial is performed.
"""
import argparse
from datetime import datetime, timezone
import importlib.util
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
from types import SimpleNamespace
import time
import uuid

sys.dont_write_bytecode=True
ROOT=Path(__file__).resolve().parents[1]
_spec=importlib.util.spec_from_file_location('alia_p01_belief_writes_integrity',ROOT/'evals/p01/belief_writes_integrity.py')
_integrity=importlib.util.module_from_spec(_spec);_spec.loader.exec_module(_integrity)
digest,read_json,strict_json=_integrity.digest,_integrity.read_json,_integrity.strict_json
PHASES=_integrity.PHASES
TOKENIZER_SHA256='19e773648cb4e65de8660ea6365e10acca112d42a854923df93db4a6f333a82d'
OWN_SOURCES=('examples/live_belief_revisions.rs','examples/support/observed_provider.rs',
    'scripts/test_live_beliefs.py','evals/p01/belief_writes_cases_v1.json',
    'evals/p01/belief_writes_integrity.py','evals/p01/test_belief_writes_integrity.py')
LEDGER_SQL="""SELECT jsonb_build_object('reservation_count',(SELECT count(*) FROM usage_reservations),
'records',(SELECT COALESCE(jsonb_agg(to_jsonb(r)), '[]'::jsonb) FROM
 (SELECT id,individual_id,run_id,window_id,mode,status,input_bound,output_cap,actual_input,actual_output,
 actual_total,charged_tokens,usage_unknown,dispatched_at,request_digest FROM usage_reservations
 ORDER BY created_at,id LIMIT 32) r))"""


def utc():return datetime.now(timezone.utc).isoformat()


def no_symlinks(path):
    path=Path(path)
    if not path.is_absolute() or '..'in path.parts:raise ValueError('absolute_nontraversing_input_required')
    for part in [*path.parents,path]:
        if part.is_symlink():raise ValueError('symlink_input_refused')
    return path


def save_report(path, report):
    temporary=path.with_name('.'+path.name+'.'+uuid.uuid4().hex+'.partial')
    try:
        with temporary.open('x',encoding='utf-8')as f:
            import json
            json.dump(report,f,ensure_ascii=False,indent=2,allow_nan=False);f.flush();os.fsync(f.fileno())
        temporary.replace(path)
    finally:temporary.unlink(missing_ok=True)


def load_frozen_qa(checkpoint):
    checkpoint=no_symlinks(checkpoint);manifest=read_json(checkpoint/'manifest.json')
    if manifest.get('format')!='alia-p03-checkpoint/1':raise ValueError('unsupported_frozen_checkpoint')
    modules={};prior={}
    try:
        for name in ('common','freeze','artifacts','database'):
            relative=f'scripts/qa/{name}.py';path=no_symlinks(checkpoint/'source'/relative)
            if manifest['sources'].get(relative)!=digest(path):raise ValueError('qa_adapter_not_bound')
            prior[name]=sys.modules.get(name)
            spec=importlib.util.spec_from_file_location(name,path);module=importlib.util.module_from_spec(spec)
            sys.modules[name]=module;spec.loader.exec_module(module);modules[name]=module
    finally:
        for name,old in prior.items():
            if old is None:sys.modules.pop(name,None)
            else:sys.modules[name]=old
    return SimpleNamespace(**modules)


def verify_inputs(args,qa):
    checkpoint=no_symlinks(args.checkpoint);binary_dir=no_symlinks(args.binary_dir)
    tokenizer=no_symlinks(args.tokenizer)
    manifest,artifacts=qa.artifacts.verify_artifacts(checkpoint,binary_dir)
    spec=_integrity.load_cases();source=checkpoint/'source'
    observer=binary_dir/'examples/live_belief_revisions'
    if not observer.is_file() or not os.access(observer,os.X_OK) or artifacts.get('files',{}).get('bin/examples/live_belief_revisions')!=digest(observer):
        raise ValueError('ninth_observer_missing_or_unbound')
    for relative,sha in spec['baseline']['core_sources'].items():
        if manifest['sources'].get(relative)!=sha or digest(no_symlinks(source/relative))!=sha:
            raise ValueError('frozen_core_binding_mismatch')
    for relative in OWN_SOURCES:
        if manifest['sources'].get(relative)!=digest(no_symlinks(source/relative)):
            raise ValueError('belief_harness_source_missing_or_damaged')
    for relative in ('scripts/test_live_beliefs.py','evals/p01/belief_writes_integrity.py','evals/p01/belief_writes_cases_v1.json'):
        if digest(ROOT/relative)!=manifest['sources'][relative]:raise ValueError('running_harness_differs_from_frozen')
    if spec['baseline']['tokenizer_sha256']!=TOKENIZER_SHA256 or digest(tokenizer)!=TOKENIZER_SHA256:raise ValueError('pinned_tokenizer_hash_mismatch')
    return {'checkpoint_id':manifest['id'],'checkpoint_sha256':digest(checkpoint/'manifest.json'),
            'artifact_manifest_sha256':digest(binary_dir.parent/'artifacts.json'),
            'observer_sha256':digest(binary_dir/'examples/live_belief_revisions'),
            'driver_sha256':digest(Path(__file__)),'checker_sha256':digest(ROOT/'evals/p01/belief_writes_integrity.py'),
            'cases_sha256':digest(ROOT/'evals/p01/belief_writes_cases_v1.json'),
            'tokenizer_sha256':digest(tokenizer),'baseline_assembly_id':spec['baseline']['assembly_id'],
            'baseline_assembly_sha256':spec['baseline']['assembly_sha256'],
            'previous_baseline_assembly_sha256':spec['baseline']['compatibility_from']['previous_assembly_sha256'],
            'preserved_criteria_sha256':_integrity.ORIGINAL_CRITERIA_SHA256,
            'core_source_count':len(spec['baseline']['core_sources']),'harness_sources':{p:manifest['sources'][p]for p in OWN_SOURCES},
            'artifact_build_commands':artifacts.get('build_commands'),'artifact_build_environment':artifacts.get('build_environment'),
            'actual_model_identity':'per-candidate persisted live mode/provider_model; missing is unverified'}


def read_key(path):
    path=no_symlinks(path)
    if path.stat().st_mode&0o077:raise ValueError('credential_file_not_private')
    keys=[];section=False
    for line in path.read_text().splitlines():
        if not line.strip()or line.strip().startswith('```'):continue
        match=re.fullmatch(r'\s*#?\s*apikey\s*[:：]\s*(.*?)\s*',line,re.I)
        if match:
            if section:
                value=match.group(1)
                if not re.fullmatch(r'''[^\s"'`]+''',value)or value.startswith('sk-proj-'):raise ValueError('invalid_explicit_go_key')
                keys.append(value)
            continue
        section=line.strip().strip('# :：`*[]').lower()=='opencode-go'
    if len(keys)!=1:raise ValueError('one_explicit_go_section_required')
    return keys[0]


def parse_stage(stdout):
    if isinstance(stdout,bytes):stdout=stdout.decode('utf-8')
    if len(stdout.encode('utf-8'))>8*1024*1024:raise ValueError('stage_projection_too_large')
    lines=[line for line in stdout.splitlines()if line.strip()]
    if len(lines)!=1:raise ValueError('one_structured_projection_required')
    value=strict_json(lines[0])
    if not isinstance(value,dict):raise ValueError('stage_object_required')
    allowed={'format','data_mode','phase','packet_version','cases_sha256','process_id','process_nonce','started_at','finished_at','worker_committed','failure','semantic_verdict','live_model_pass','before','after','input_accounting','submitted','conversation_id','worker_outcome','provider_observations','commits','reviews','replies','decision_events','run','diagnostics','ledger_snapshot'}
    if not set(value)<=allowed:raise ValueError('unexpected_collector_fields_refused')
    return value


def recover_ledger(qa,state,database):
    try:
        ledger=strict_json(qa.database.sql(state,LEDGER_SQL,database))
        summary=_integrity.check_ledger(ledger)
        return {'recovered':True,**ledger,**summary}
    except (OSError,ValueError,TypeError,KeyError,RuntimeError,subprocess.SubprocessError):
        return {'recovered':False,'complete':False,'charged_tokens':None,'observed_ledger_charged_tokens':None,
                'issues':['final_ledger_unavailable'],'monetary_cost':None}


def execute(args,qa,report,output,lock_fd,run=subprocess.run):
    """Exactly one parent lifetime, one disposable DB, four child invocations max."""
    state_path=None;state=None;provision_attempted=False;database='alia_live_test_'+uuid.uuid4().hex;create_attempted=False
    report.update(data_mode='synthetic_inputs_real_provider',job_id=args.job_id,
                  resource_cleanup={'status':'not_started'},trial_database_cleanup={'status':'not_started'})
    def fail(code,phase=None):
        if report['failure']is None:report['failure']={'code':code,'phase':phase}
    def guard():
        if state is None or state.get('role')!='behavior' or state.get('status')!='ready' or not state.get('container_id')or not state.get('image_id'):raise ValueError('owned_behavior_resource_not_ready')
        current=qa.database.load_state(state_path)
        for key in ('owner','role','container','container_id','volume','image_id','port'):
            if current.get(key)!=state.get(key):raise ValueError('resource_identity_changed')
        if any(qa.database.assert_owned(state,kind)is None for kind in ('container','volume')):raise ValueError('exact_owned_resource_absent')
    save_report(output,report)
    try:
        directory=no_symlinks(args.db_work_dir)
        directory.mkdir(mode=0o700) # exclusive per job, refuse even an empty old directory
        key=read_key(args.credentials)
        provision_attempted=True
        state_path,state=qa.database.provision(args.checkpoint/'source','behavior',directory)
        guard();report['test_resource']=qa.database.public(state);save_report(output,report)
        create_attempted=True
        qa.database.sql(state,'CREATE DATABASE '+database,'postgres')
        env=qa.common.clean_env()
        env.update(DATABASE_URL=qa.database.url(state,database),OPENCODE_API_KEY=key,
                   ALIA_MODEL_PROVIDER='opencode_go',ALIA_MODEL_ENDPOINT='https://opencode.ai/inference/go/openai/v1',
                   ALIA_MODEL='glm-5.3-flash',ALIA_TOKENIZER_PATH=str(args.tokenizer),ALIA_QA_LOCK_FD=str(lock_fd))
        for phase in PHASES:
            stage={'phase':phase,'status':'running','exit':None,'started_at':utc(),'integrity':None}
            report['stages'].append(stage);save_report(output,report);started=time.monotonic()
            try:
                verify_inputs(args,qa);guard()
                child=run([str(args.binary_dir/'examples/live_belief_revisions')],cwd=args.checkpoint/'source',
                          env=dict(env,ALIA_BELIEF_PHASE=phase),capture_output=True,text=True,timeout=180,pass_fds=(lock_fd,))
                stage['exit']=child.returncode
                value=parse_stage(child.stdout);report['reports'].append(value)
                if value.get('data_mode')!='synthetic_inputs_real_provider':fail('fixture_not_actual_runtime',phase)
                stage['integrity']=_integrity.check_stage(value,report['reports'][-2]if len(report['reports'])>1 else None)
                if value.get('phase')!=phase:fail('phase_projection_mismatch',phase)
                if child.returncode!=0:fail('observer_nonzero',phase)
                if not stage['integrity']['machine_inventory_ready']:fail('stage_inventory_incomplete',phase)
            except subprocess.TimeoutExpired:fail('observer_timeout',phase)
            except (OSError,ValueError,TypeError,KeyError,RuntimeError,UnicodeError,subprocess.SubprocessError):fail('observer_or_input_capture_failed',phase)
            stage.update(status='failed'if report['failure']else 'passed',finished_at=utc(),elapsed_seconds=round(time.monotonic()-started,3))
            save_report(output,report)
            print('Belief stage '+phase+': '+stage['status'],flush=True)
            if report['failure']:break
    except qa.database.ProvisionError as error:
        state_path=error.state_path;fail('owned_resource_startup_failed')
    except KeyboardInterrupt:fail('parent_interrupted')
    except (OSError,ValueError,TypeError,KeyError,RuntimeError,subprocess.SubprocessError):fail('parent_operation_failed')
    finally:
        for unfinished in report['stages']:
            if unfinished['status']=='running':unfinished.update(status='failed',finished_at=utc())
        if state_path is None and provision_attempted:
            recovery=args.db_work_dir/'behavior.json'
            if recovery.is_file():state_path=recovery
        # Do not lose settled/unknown fees when child output or a later check fails.
        if create_attempted and state is not None:
            report['accounting']=recover_ledger(qa,state,database)
            if not report['accounting']['complete']:fail('final_accounting_incomplete')
            save_report(output,report)
            try:
                guard();qa.database.sql(state,'DROP DATABASE IF EXISTS '+database+' WITH (FORCE)','postgres')
                report['trial_database_cleanup']={'status':'passed'}
            except (OSError,ValueError,TypeError,KeyError,RuntimeError,subprocess.SubprocessError):
                report['trial_database_cleanup']={'status':'failed'};fail('trial_database_cleanup_failed')
        elif state_path is None:report['trial_database_cleanup']={'status':'not_started'}
        if state_path is not None:
            try:
                owned=qa.database.load_state(state_path)
                if Path(state_path).parent!=args.db_work_dir or owned.get('role')!='behavior':raise ValueError('cleanup_state_not_owned_by_job')
                if state is not None and any(owned.get(k)!=state.get(k)for k in ('owner','container','volume')):raise ValueError('cleanup_identity_changed')
                cleanup=qa.database.cleanup(state_path)
                cleaned=qa.database.load_state(state_path)
                absence={kind:qa.database.assert_owned(cleaned,kind)is None for kind in ('container','volume')}
                absent=all(absence.values())
                report['resource_cleanup']={'status':'passed'if cleanup.get('passed')is True and absent else 'failed',
                    'exact_container_absent':absence['container'],'exact_volume_absent':absence['volume'],'password_removed':'password'not in cleaned,'private_state_status':cleaned.get('status')}
                if report['resource_cleanup']['status']!='passed' or 'password'in cleaned:fail('owned_resource_cleanup_incomplete')
            except (OSError,ValueError,TypeError,KeyError,RuntimeError,subprocess.SubprocessError):
                report['resource_cleanup']={'status':'failed','code':'exact_owned_cleanup_failed'};fail('owned_resource_cleanup_failed')
        else:report['resource_cleanup']={'status':'not_started'}
        try:
            current=verify_inputs(args,qa)
            if current!=report['provenance']:raise ValueError('frozen_inputs_changed')
            report['final_verification']={'status':'passed'}
        except (OSError,ValueError,TypeError,KeyError,RuntimeError):
            report['final_verification']={'status':'failed'};fail('final_frozen_verification_failed')
        report['integrity']=_integrity.check_suite(report['reports'])
        if not report['integrity']['machine_inventory_ready']:fail('four_stage_inventory_incomplete')
        if len(report['reports'])==4 and report['accounting'].get('recovered'):
            runs=[r['submitted']['operation_id']for r in report['reports']if 'submitted'in r]
            final=_integrity.check_ledger(report['accounting'],runs)
            report['accounting'].update(final)
            if not final['complete']:fail('final_ledger_run_coverage')
            rows={r['id']:r for r in report['accounting']['records']}
            for projection in report['reports']:
                for row in projection.get('ledger_snapshot',{}).get('records',[]):
                    if rows.get(row['id'])!=row:fail('cumulative_ledger_snapshot_conflict')
        report['not_run_phases']=list(PHASES[len(report['stages']):])
        report['engine_pass']=report['failure']is None and len(report['stages'])==4 and report['accounting'].get('complete')is True and report['resource_cleanup']['status']=='passed' and report['trial_database_cleanup']['status']=='passed'
        report['finished_at']=utc();save_report(output,report)
    return report


def main(argv=None,*,run=subprocess.run,qa_loader=load_frozen_qa):
    parser=argparse.ArgumentParser(description=__doc__)
    for name in ('checkpoint','binary-dir','tokenizer','output'):parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--execute',action='store_true');parser.add_argument('--job-id')
    parser.add_argument('--credentials',type=Path);parser.add_argument('--db-work-dir',type=Path)
    args=parser.parse_args(argv)
    if args.execute and (not args.credentials or not args.db_work_dir or not args.job_id or not re.fullmatch('[A-Z0-9][A-Z0-9-]{2,99}',args.job_id)):
        parser.error('--execute requires the root-released --job-id, explicit --credentials and new private --db-work-dir')
    output=no_symlinks(args.output);output.parent.mkdir(parents=True,exist_ok=True)
    try:
        with output.open('x')as f:f.write('{}\n')
    except FileExistsError:parser.error('Report exists; preserve it and choose a fresh --output')
    report={'version':'alia-p01-belief-trial/1','started_at':utc(),'engine_pass':False,
            'semantic_verdict':'unverified','live_model_pass':None,'behavior_pass':None,'stages':[],'reports':[],
            'failure':None,'accounting':{'recovered':False,'complete':False,'charged_tokens':None},
            'data_mode':'preparation_only','not_run_phases':list(PHASES),'execution_authorization':'Complete M5 engineering qualification then a separately named root release required; B03 is preparation only. CLI job ID is provenance, not permission.'}
    save_report(output,report)
    try:
        for name in ('checkpoint','binary_dir','tokenizer'):no_symlinks(getattr(args,name))
        qa=qa_loader(args.checkpoint)
        with qa.common.heavy_lock()as (fd,queue):
            report['provenance']=verify_inputs(args,qa);report['queue_seconds']=round(queue,3)
            if args.execute:
                no_symlinks(args.db_work_dir);no_symlinks(args.credentials)
                old_term=signal.getsignal(signal.SIGTERM)
                def interrupted(_number,_frame):raise KeyboardInterrupt()
                signal.signal(signal.SIGTERM,interrupted)
                try:report=execute(args,qa,report,output,fd,run=run)
                finally:signal.signal(signal.SIGTERM,old_term)
            else:
                report.update(preparation_ready=True,model_calls=0,db_calls=0,finished_at=utc())
                save_report(output,report)
                print('Frozen file preparation ready; no DB/model trial executed')
                return 0
    except (OSError,ValueError,TypeError,KeyError,RuntimeError,ImportError,KeyboardInterrupt)as error:
        safe_codes={'absolute_nontraversing_input_required','symlink_input_refused','unsupported_frozen_checkpoint','qa_adapter_not_bound','ninth_observer_missing_or_unbound','frozen_core_binding_mismatch','belief_harness_source_missing_or_damaged','running_harness_differs_from_frozen','pinned_tokenizer_hash_mismatch','frozen_case_hash_mismatch','original_belief_criteria_changed'}
        code=str(error)if isinstance(error,ValueError)and str(error)in safe_codes else 'explicit_preflight_or_finalization_failed'
        report['failure']=report['failure']or {'code':code,'phase':None}
        report['engine_pass']=False;report['finished_at']=utc();save_report(output,report)
        print('Explicit belief preparation/trial failed; private details withheld')
        return 1
    print('Belief trial engineering PASS; semantic review unverified'if report['engine_pass']else 'Belief trial engineering FAILED; inspect report')
    return 0 if report['engine_pass']else 1


if __name__=='__main__':raise SystemExit(main())
