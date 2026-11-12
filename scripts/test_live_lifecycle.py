#!/usr/bin/env python3
"""Bounded live examples, synthetic data and a disposable database only.

This is the existing lifecycle orchestrator, not a new individual/goal executor.
Stdout is only the examples' structured synthetic projection; stderr is withheld.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import os
import re
from pathlib import Path
import subprocess
import sys
from types import SimpleNamespace
import time
import uuid
from probe_model import read_go_key

ROOT = Path(__file__).resolve().parents[1]
PHASES = ('first-study', 'restore', 'observe', 'unknown')
BINARIES = {'uncertainty': 'live_uncertainty', 'grounding': 'live_grounding',
            'person-retrieval': 'live_person_retrieval', 'entities': 'live_entities',
            'discover': 'live_autonomy', 'observe': 'live_autonomy'}
SPECIAL = ('observe', 'discover', 'entities', 'person-retrieval', 'grounding', 'uncertainty')
EXCERPT_CRITERION = '保存指定资料片段的逐字摘录，并保留读取与笔记来源。'
SOURCE_FILES = ('Cargo.lock', 'config/persona.example.json', 'config/runtime.example.json',
                'examples/live_individual.rs', 'examples/live_autonomy.rs',
                'examples/support/lifecycle.rs', 'examples/support/observed_provider.rs',
                'schemas/decision.schema.json', 'schemas/decision-assessment.schema.json',
                'schemas/goal-assessment.schema.json', 'scripts/test_live_lifecycle.py')


def strict_json(value):
    def pairs(items):
        out = {}
        for key, item in items:
            if key in out:
                raise ValueError('duplicate_key')
            out[key] = item
        return out
    return json.loads(value, object_pairs_hook=pairs,
                      parse_constant=lambda _: (_ for _ in ()).throw(ValueError('nonfinite')))


def sha256(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()


def utc_now():
    return datetime.now(timezone.utc).isoformat()


def save_report(path, report):
    # The output is reserved by main with O_EXCL. Atomic replacement prevents
    # readers observing half JSON during a running trial or final cleanup.
    temp = path.with_name(path.name + '.partial')
    with temp.open('w', encoding='utf-8') as stream:
        json.dump(report, stream, ensure_ascii=False, indent=2, allow_nan=False)
        stream.flush()
        os.fsync(stream.fileno())
    temp.replace(path)


def parse_projection(stdout):
    """Keep every complete object even if adjacent lines are damaged; no raw log."""
    if isinstance(stdout, bytes):
        try:
            stdout = stdout.decode('utf-8')
        except UnicodeDecodeError:
            return [], [{'code': 'invalid_utf8'}]
    objects, errors = [], []
    lines = (stdout or '').splitlines()
    for number, line in enumerate(lines, 1):
        if not line.strip():
            continue
        try:
            value = strict_json(line)
            if not isinstance(value, dict):
                raise ValueError('object_required')
            objects.append(value)
        except (ValueError, TypeError):
            errors.append({'code': 'invalid_json_line', 'line': number})
    if not objects and not errors:
        errors.append({'code': 'no_json'})
    if len(objects) > 1:
        errors.append({'code': 'multiple_reports', 'count': len(objects)})
    return objects, errors


def phase_checks(phase, report, reports):
    """Engineering checks only; no word-matching or model semantic verdict."""
    errors = []
    def check(ok, code):
        if not ok:
            errors.append(code)
    check(report['phase'] == phase, 'phase_mismatch')
    check(bool(report['individual_id']) and report['individual_id'] == reports[0]['individual_id'], 'identity_changed')
    check(report['budget_frozen'] is False, 'budget_frozen_or_unknown')
    if phase == 'first-study':
        check(bool(report['goals']), 'research_goal_missing')
        check(any(o['status'] == 'succeeded' and o['capability_id'] == 'documents.read'
                  for o in report['observations']), 'actual_reading_missing')
        check(any(m['scope'] == 'general' for m in report['memories']), 'general_memory_missing')
        check(all(g['completion'] is None and g['criterion'] != EXCERPT_CRITERION
                  for g in report['goals']), 'research_narrowed')
    if phase in ('restore', 'observe'):
        original = {g['id']: g['criterion'] for g in reports[0]['goals']}
        current = {g['id']: g['criterion'] for g in report['goals']}
        check(all(current.get(key) == criterion for key, criterion in original.items()), 'original_goal_changed')
    if phase == 'discover':
        check(not any(e['kind'] == 'message.received' for e in report['events']), 'discovery_user_input')
    if phase in SPECIAL:
        check(report['engine_pass'] is True, 'example_engine_failed')
    else:
        check(bool(report['replies']), 'committed_reply_missing')
    return errors


def collect_provenance(binary_dir, phases, checkpoint=None, work_root=ROOT):
    binaries = {name: sha256(binary_dir / name)
                for name in sorted({BINARIES.get(p, 'live_individual') for p in phases})}
    sources = {name: sha256(work_root / name) for name in SOURCE_FILES if (work_root / name).is_file()}
    result = {'binary_dir': str(binary_dir), 'binaries': binaries, 'sources': sources,
              'driver_source_note': 'Current checkout hashes are observational; only coordinator checkpoint binds build sources',
              'checkpoint_verified': False,
              'requested_model': {'provider': 'opencode_go', 'model': 'glm-5.3-flash'},
              'actual_model_identity': 'see per-stage models/provider_model; missing is unknown'}
    if checkpoint is not None:
        manifest = strict_json(checkpoint.read_text())
        # A coordinator's frozen manifest binds artifacts to its build snapshot.
        # Source hashes describe that snapshot, which may differ from the active
        # checkout. We do not infer a binary's source from current file mtimes.
        checkpoint_id = manifest.get('id') or manifest.get('format')
        checkpoint_sources = manifest.get('sources', manifest.get('files'))
        if not isinstance(checkpoint_id, str) or not checkpoint_id:
            raise ValueError('checkpoint_id_missing')
        if not checkpoint_sources or not isinstance(checkpoint_sources, dict):
            raise ValueError('checkpoint_sources_missing')
        if any(not isinstance(k, str) or not isinstance(v, str) or not re.fullmatch(r'[0-9a-f]{64}', v)
               for k, v in checkpoint_sources.items()):
            raise ValueError('checkpoint_source_hash_invalid')
        if any(manifest.get('binaries', {}).get(k) != v for k, v in binaries.items()):
            raise ValueError('checkpoint_binary_mismatch')
        result.update(checkpoint_verified=True, checkpoint_id=checkpoint_id,
                      checkpoint_sha256=sha256(checkpoint), checkpoint=manifest)
    return result


def recover_accounting(psql, database, run=subprocess.run, work_root=ROOT):
    recovery = list(psql)
    recovery[recovery.index('-d') + 1] = database
    query = """SELECT jsonb_build_object(
      'reservation_count',(SELECT count(*) FROM usage_reservations),
      'usage',(SELECT COALESCE(jsonb_agg(to_jsonb(u)), '[]'::jsonb) FROM
        (SELECT id,run_id,status,actual_input,actual_output,charged_tokens,usage_unknown
         FROM usage_reservations ORDER BY created_at,id LIMIT 256) u),
      'budget_frozen',(SELECT budget_frozen FROM individuals WHERE singleton))"""
    metadata = {'recovered': False, 'scope': 'actual disposable trial database',
                'limit': 256, 'charged_tokens': None, 'usage_complete': False}
    try:
        result = run(recovery[:-1] + ['-At', '-c', query], cwd=work_root,
                     capture_output=True, text=True, timeout=15)
        if result.returncode:
            return dict(metadata, error_code='accounting_query_failed', exit=result.returncode)
        data = strict_json(result.stdout)
        rows, count = data['usage'], data['reservation_count']
        if not isinstance(rows, list) or type(count) is not int or count < len(rows):
            raise ValueError('accounting_shape')
        for row in rows:
            if not isinstance(row['id'], str) or not isinstance(row['run_id'], str):
                raise ValueError('accounting_identity')
            if row['status'] not in ('reserved', 'dispatched', 'settled'):
                raise ValueError('accounting_status')
            if row['charged_tokens'] is not None and (type(row['charged_tokens']) is not int or row['charged_tokens'] < 0):
                raise ValueError('accounting_charge')
            if type(row['usage_unknown']) is not bool:
                raise ValueError('accounting_unknown')
        if len({r['id'] for r in rows}) != len(rows):
            raise ValueError('accounting_duplicate')
        # Unknown/dispatched is never zero. Keep observed ledger charges apart
        # from provider-known actual usage and from monetary prices (unavailable).
        complete = len(rows) == count
        known = complete and all(r['status'] == 'settled' and not r['usage_unknown']
                                 and type(r['charged_tokens']) is int for r in rows)
        allowed = [{k: row[k] for k in ('id', 'run_id', 'status', 'actual_input',
                    'actual_output', 'charged_tokens', 'usage_unknown')} for row in rows]
        return dict(metadata, recovered=True, reservation_count=count, usage=allowed,
                    usage_truncated=not complete, usage_complete=known,
                    budget_frozen=data['budget_frozen'],
                    observed_ledger_charged_tokens=sum(r['charged_tokens'] for r in rows if type(r['charged_tokens']) is int),
                    charged_tokens=sum(r['charged_tokens'] for r in rows) if known else None,
                    monetary_cost=None)
    except subprocess.TimeoutExpired:
        return dict(metadata, error_code='accounting_timeout')
    except (OSError, ValueError, KeyError, TypeError, RuntimeError):
        return dict(metadata, error_code='accounting_unavailable')


def execute_trial(phases, binary_dir, output, env, psql, database, provenance, run=subprocess.run, work_root=ROOT, final_verify=None):
    if not re.fullmatch(r'alia_live_test_[0-9a-f]{32}', database):
        raise ValueError('refusing non-disposable database')
    report = {'report_version': 'alia-p01/1', 'started_at': utc_now(),
              'data_mode': 'synthetic_inputs_real_provider', 'provenance': provenance,
              'planned_phases': list(phases), 'reports': [], 'stages': [], 'latest_exit': None,
              'engine_pass': False, 'behavior_review': 'unverified; independent source comparison required',
              'behavior_pass': None, 'failure': None, 'cleanup': {'status': 'not_started'},
              'accounting': {'recovered': False, 'charged_tokens': None}}
    created = False
    create_attempted = False
    def fail(category, code, phase=None):
        if report['failure'] is None:
            report['failure'] = {'category': category, 'code': code, 'phase': phase}
    save_report(output, report)
    try:
        create_attempted = True
        result = run(psql + [f'CREATE DATABASE {database}'], cwd=work_root, capture_output=True, text=True, timeout=30)
        if result.returncode:
            fail('driver_fault', 'database_create_failed')
        else:
            created = True
        for phase in phases if created else ():
            stage = {'phase': phase, 'started_at': utc_now(), 'status': 'running',
                     'exit': None, 'parse_errors': [], 'engineering_errors': []}
            report['stages'].append(stage)
            report['latest_exit'] = None
            save_report(output, report)
            stage_env = dict(env, ALIA_LIVE_PHASE=phase)
            started = time.monotonic()
            objects = []
            try:
                result = run([str(binary_dir / BINARIES.get(phase, 'live_individual'))],
                             cwd=work_root, env=stage_env, capture_output=True, text=True,
                             timeout=1000 if phase in ('observe', 'discover', 'person-retrieval', 'grounding', 'uncertainty') else 180)
                stage['exit'] = report['latest_exit'] = result.returncode
                objects, stage['parse_errors'] = parse_projection(result.stdout)
            except subprocess.TimeoutExpired as error:
                objects, stage['parse_errors'] = parse_projection(error.stdout)
                fail('driver_fault', 'phase_timeout', phase)
            except (OSError, UnicodeError, ValueError, RuntimeError):
                fail('driver_fault', 'phase_process_failed', phase)
            report['reports'].extend(objects)
            stage['elapsed_seconds'] = round(time.monotonic() - started, 3)
            stage['finished_at'] = utc_now()
            if stage['parse_errors']:
                fail('driver_fault', 'phase_output_invalid', phase)
            if stage['exit'] not in (0, None):
                fail('runtime_failure', 'phase_exit_nonzero', phase)
            if report['failure'] is None:
                try:
                    stage['engineering_errors'] = phase_checks(phase, objects[0], report['reports'])
                except (KeyError, TypeError, IndexError):
                    fail('driver_fault', 'projection_shape_invalid', phase)
                if stage['engineering_errors']:
                    fail('runtime_failure', 'phase_engineering_failed', phase)
            stage['status'] = 'failed' if report['failure'] else 'passed'
            save_report(output, report)
            print(json.dumps({'phase': phase, 'exit': stage['exit'], 'status': stage['status'], 'report': str(output)}), flush=True)
            if report['failure']:
                break
        if created and report['failure'] is None and tuple(phases) == PHASES:
            conversations = [r['replies'][0]['conversation_id'] for r in report['reports'] if r['phase'] != 'observe']
            if len(set(conversations)) != 3:
                fail('runtime_failure', 'foreground_session_reused')
    except KeyboardInterrupt:
        fail('driver_fault', 'driver_interrupted')
    except subprocess.TimeoutExpired:
        fail('driver_fault', 'database_create_timeout')
    except (OSError, ValueError, KeyError, TypeError, IndexError, RuntimeError):
        fail('driver_fault', 'driver_operation_failed')
    finally:
        for unfinished in report['stages']:
            if unfinished['status'] == 'running':
                unfinished['status'] = 'failed'
                unfinished['finished_at'] = utc_now()
        # Read the same final ledger on success and failure BEFORE cleanup.
        # A create timeout may have created a database, so recover/clean it too.
        if create_attempted:
            report['accounting'] = recover_accounting(psql, database, run, work_root)
            if report['failure']:
                report['failure_accounting'] = report['accounting']
            if not report['accounting']['recovered'] or not report['accounting']['usage_complete']:
                fail('driver_fault', 'accounting_incomplete')
            save_report(output, report)
            try:
                cleanup = run(psql + [f'DROP DATABASE IF EXISTS {database} WITH (FORCE)'],
                              cwd=work_root, capture_output=True, text=True, timeout=30)
                report['cleanup'] = {'status': 'passed' if cleanup.returncode == 0 else 'failed', 'exit': cleanup.returncode}
            except subprocess.TimeoutExpired:
                report['cleanup'] = {'status': 'failed', 'code': 'cleanup_timeout'}
            except (OSError, ValueError, RuntimeError):
                report['cleanup'] = {'status': 'failed', 'code': 'cleanup_process_failed'}
            if report['cleanup']['status'] != 'passed':
                fail('driver_fault', 'cleanup_failed')
        if final_verify is not None:
            try:
                final_verify()
                report['final_verification'] = {'status': 'passed'}
            except (OSError, ValueError, KeyError, TypeError, RuntimeError):
                report['final_verification'] = {'status': 'failed', 'code': 'final_verification_failed'}
                fail('driver_fault', 'final_verification_failed')
        report['not_run_phases'] = list(phases[len(report['stages']):])
        report['engine_pass'] = report['failure'] is None and len(report['stages']) == len(phases)
        report['finished_at'] = utc_now()
        save_report(output, report)
    return report


def no_symlink_components(path):
    path = Path(path).absolute()
    for component in [*path.parents, path]:
        if component.is_symlink():
            raise ValueError('symlink path component refused')
    return path


def load_frozen_qa(checkpoint):
    """Load P03 adapters only after binding their bytes to the source manifest.

    Full inventory/artifact verification is then delegated to those adapters.
    No changing checkout helper or implicit fallback participates in explicit mode.
    """
    checkpoint = no_symlink_components(checkpoint)
    manifest = strict_json((checkpoint / 'manifest.json').read_text())
    if manifest.get('format') != 'alia-p03-checkpoint/1':
        raise ValueError('unsupported explicit checkpoint')
    source = checkpoint / 'source'
    names = ('common', 'freeze', 'artifacts', 'database')
    previous, modules = {}, {}
    old_bytecode = sys.dont_write_bytecode
    sys.dont_write_bytecode = True
    try:
        for name in names:
            relative = f'scripts/qa/{name}.py'
            path = no_symlink_components(source / relative)
            if manifest.get('sources', {}).get(relative) != sha256(path):
                raise ValueError('unbound QA adapter')
            previous[name] = sys.modules.get(name)
            spec = importlib.util.spec_from_file_location(name, path)
            module = importlib.util.module_from_spec(spec)
            sys.modules[name] = module
            spec.loader.exec_module(module)
            modules[name] = module
    finally:
        sys.dont_write_bytecode = old_bytecode
        for name, old in previous.items():
            if old is None:
                sys.modules.pop(name, None)
            else:
                sys.modules[name] = old
    return SimpleNamespace(**modules)


def failure_report(output, phases, code='preflight_failed', provenance=None):
    save_report(output, {'report_version': 'alia-p01/1', 'engine_pass': False, 'behavior_pass': None,
                         'behavior_review': 'unverified', 'failure': {'category': 'driver_fault', 'code': code},
                         'planned_phases': list(phases), 'not_run_phases': list(phases), 'reports': [], 'stages': [],
                         'provenance': provenance or {},
                         'accounting': {'recovered': False, 'charged_tokens': None, 'scope': 'no trial process started'},
                         'cleanup': {'status': 'not_needed'}})


def run_explicit(args, phases, output, run=subprocess.run, qa_loader=load_frozen_qa):
    if args.checkpoint is None or args.binary_dir is None or args.test_db_state is None:
        raise ValueError('explicit inputs missing')
    if not all(path.is_absolute() for path in (args.checkpoint, args.binary_dir, args.test_db_state)):
        raise ValueError('explicit inputs must be absolute')
    checkpoint = no_symlink_components(args.checkpoint)
    binary_dir = no_symlink_components(args.binary_dir)
    private_state = no_symlink_components(args.test_db_state)
    qa = qa_loader(checkpoint)
    # The actual lock adapter validates inherited descriptors or holds its own
    # descriptor continuously through create, stages, accounting and cleanup.
    with qa.common.heavy_lock() as (_, queue_seconds):
        manifest, artifacts = qa.artifacts.verify_artifacts(checkpoint, binary_dir)
        source = checkpoint / 'source'
        expected_driver = manifest.get('sources', {}).get('scripts/test_live_lifecycle.py')
        if expected_driver != sha256(Path(__file__)):
            raise ValueError('driver differs from checkpoint')
        state = qa.database.load_state(private_state)
        if state.get('format') != 'alia-p03-db/1' or state.get('role') != 'behavior' or state.get('status') != 'ready':
            raise ValueError('behavior test DB state required')
        if not state.get('container_id') or not state.get('image_id'):
            raise ValueError('test DB identity missing')
        def guard():
            if qa.database.assert_owned(state, 'container') is None or qa.database.assert_owned(state, 'volume') is None:
                raise ValueError('owned behavior resource absent')
        guard()
        def guarded_run(command, **kwargs):
            guard()
            return run(command, **kwargs)
        examples = binary_dir / 'examples'
        provenance = collect_provenance(examples, phases, work_root=source)
        provenance.update(checkpoint_verified=True, checkpoint_id=manifest['id'],
                          checkpoint_sha256=sha256(checkpoint / 'manifest.json'),
                          artifact_manifest_sha256=sha256(binary_dir.parent / 'artifacts.json'),
                          checkpoint=manifest, artifact_build=artifacts.get('build_commands'),
                          artifact_build_environment=artifacts.get('build_environment'),
                          artifact_tool_versions_log=artifacts.get('tool_versions_log'),
                          test_db=qa.database.public(state), queue_seconds=round(queue_seconds, 3),
                          driver_sha256=sha256(Path(__file__)))
        provenance['scenario'] = {'phases': list(phases), 'discovery_source': args.discovery_scenario,
                                  'first_study_idle_check_minutes': 5, 'clock': 'real scheduling; no clock writes'}
        database = 'alia_live_test_' + uuid.uuid4().hex
        env = qa.common.clean_env()
        # The tokenizer is an explicitly selected verified model asset, not a
        # default .env fallback. The runtime still checks its pinned fingerprint.
        tokenizer = os.environ.get('ALIA_TOKENIZER_PATH')
        if tokenizer:
            env['ALIA_TOKENIZER_PATH'] = tokenizer
            provenance['tokenizer_sha256'] = sha256(Path(tokenizer))
        env.update(DATABASE_URL=qa.database.url(state, database),
                   ALIA_MODEL_PROVIDER='opencode_go', ALIA_MODEL_ENDPOINT='https://opencode.ai/inference/go/openai/v1',
                   ALIA_MODEL='glm-5.3-flash', OPENCODE_API_KEY=read_go_key(args.credentials.read_text()),
                   ALIA_LIVE_SOURCE_SCENARIO=args.discovery_scenario)
        psql = [*qa.database.DOCKER, 'exec', '-i', state['container'], 'psql', '-X', '-U',
                state['username'], '-d', 'postgres', '-v', 'ON_ERROR_STOP=1', '-c']
        def final_verify():
            qa.artifacts.verify_artifacts(checkpoint, binary_dir)
            guard()
        return execute_trial(phases, examples, output, env, psql, database, provenance,
                             run=guarded_run, work_root=source, final_verify=final_verify)


def main(argv=None, *, run=subprocess.run, qa_loader=load_frozen_qa):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--credentials', type=Path, required=True)
    parser.add_argument('--output', type=Path, default=ROOT / 'evals/results/live-lifecycle.json')
    parser.add_argument('--binary-dir', type=Path, help='Legacy examples directory, or explicit artifacts/bin')
    parser.add_argument('--checkpoint', type=Path, help='Legacy manifest file, or explicit immutable checkpoint directory')
    parser.add_argument('--test-db-state', type=Path, help='Explicit mode: private behavior-role resource JSON')
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument('--discovery-only', action='store_true')
    modes.add_argument('--entities-only', action='store_true')
    modes.add_argument('--retrieval-only', action='store_true')
    modes.add_argument('--grounding-only', action='store_true')
    modes.add_argument('--uncertainty-only', action='store_true')
    parser.add_argument('--discovery-scenario', choices=('facts', 'gap'), default='facts')
    args = parser.parse_args(argv)
    if not args.discovery_only and args.discovery_scenario != 'facts':
        parser.error('--discovery-scenario gap requires --discovery-only')
    phases = next(((phase,) for selected, phase in (
        (args.uncertainty_only, 'uncertainty'), (args.grounding_only, 'grounding'),
        (args.retrieval_only, 'person-retrieval'), (args.entities_only, 'entities'),
        (args.discovery_only, 'discover')) if selected), PHASES)
    output = args.output.resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    try:
        with output.open('x', encoding='utf-8') as stream:
            stream.write('{}\n')
    except FileExistsError:
        parser.error('Report exists; choose a fresh --output path to preserve evidence')
    explicit = args.test_db_state is not None or (args.checkpoint is not None and not args.checkpoint.is_file())
    if explicit:
        try:
            report = run_explicit(args, phases, output, run=run, qa_loader=qa_loader)
        except (OSError, ValueError, KeyError, TypeError, RuntimeError, ImportError):
            # Post-start failures are already captured by execute_trial; a
            # final-verification failure must never erase its ledger/report.
            previous = strict_json(output.read_text())
            if previous.get('report_version') != 'alia-p01/1':
                failure_report(output, phases, 'explicit_preflight_failed')
            else:
                previous['engine_pass'] = False
                previous.setdefault('failure', None)
                if previous['failure'] is None:
                    previous['failure'] = {'category': 'driver_fault', 'code': 'explicit_finalization_failed'}
                save_report(output, previous)
            print('Explicit lifecycle failed; inspect structured report (private details withheld)')
            return 1
    else:
        try:
            binary_dir = (args.binary_dir or ROOT / 'target/debug/examples').resolve()
            provenance = collect_provenance(binary_dir, phases, args.checkpoint)
            settings = dict(line.split('=', 1) for line in (ROOT / '.env').read_text().splitlines()
                            if line.strip() and not line.startswith('#'))
            database = 'alia_live_test_' + uuid.uuid4().hex
            env = os.environ.copy()
            env.update(DATABASE_URL=f"postgres://alia:{settings['ALIA_DB_PASSWORD']}@127.0.0.1:{settings.get('ALIA_DB_PORT', '55433')}/{database}",
                       ALIA_MODEL_PROVIDER='opencode_go', ALIA_MODEL_ENDPOINT='https://opencode.ai/inference/go/openai/v1',
                       ALIA_MODEL='glm-5.3-flash', OPENCODE_API_KEY=read_go_key(args.credentials.read_text()),
                       ALIA_LIVE_SOURCE_SCENARIO=args.discovery_scenario)
        except (OSError, ValueError, KeyError, TypeError):
            failure_report(output, phases)
            print('Lifecycle preflight failed; private details withheld')
            return 1
        provenance['scenario'] = {'phases': list(phases), 'discovery_source': args.discovery_scenario,
                                  'first_study_idle_check_minutes': 5, 'clock': 'real scheduling; no clock writes',
                                  'source': 'existing Rust examples, bound by checkpoint'}
        psql = ['sudo', '-n', 'docker', 'compose', 'exec', '-T', 'db', 'psql', '-U', 'alia', '-d', 'postgres', '-v', 'ON_ERROR_STOP=1', '-c']
        report = execute_trial(phases, binary_dir, output, env, psql, database, provenance, run=run)
    print('Lifecycle engineering PASS; behavior remains unverified' if report['engine_pass'] else 'Lifecycle engineering FAILED; inspect structured report')
    return 0 if report['engine_pass'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
