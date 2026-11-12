#!/usr/bin/env python3
"""Source-bound SQLx/Docker dispatch and an independently admitted sandbox stage."""
import argparse
from collections import Counter
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import tempfile
import time

from common import clean_env, digest, resource_locks, read_json, utc, write_new
from freeze import verify, inventory

SDK = 'src/adapters/sandbox.rs'
TEST = 'tests/sandbox.rs'
IMAGE = 'python@sha256:a5a7017efc5969e00ae2b91c8eb0631049a569652aa4ffc4501769049ae45c72'
DOCKER = ['/usr/bin/sudo', '-n', '/usr/bin/docker']
CASES = frozenset(('actual_foreign_identity_is_not_removed_by_owned_cleanup',
    'unavailable_native_launcher_never_escalates_to_sudo_or_executes_source',
    'normal_computation_stdin_and_clean_environment_are_actual',
    'maximum_utf8_source_uses_stdin_and_real_nonzero_exit_is_code_failed',
    'filesystem_tmpfs_noexec_and_no_host_data_or_socket_are_actual',
    'network_tcp_and_dns_have_real_host_positive_controls_and_cannot_reach_them',
    'real_cgroups_bound_processes_memory_swap_and_cpu',
    'infinite_time_and_output_are_actually_stopped_with_bounded_evidence',
    'missing_image_is_unavailable_without_pull_or_execution_fallback'))
OUTCOMES = {'normal':'succeeded', 'maximum_source':'succeeded', 'code_failed':'code_failed',
    'filesystem':'succeeded', 'network':'succeeded', 'cgroup_process_cpu':'succeeded',
    'memory_oom':'resource_exceeded', 'timed_out':'timed_out', 'output_limit':'output_limit',
    'invalid_utf8_output':'output_limit', 'unavailable_image':'unavailable', 'native_unavailable':'unavailable'}
NAME = re.compile(r'alia-sandbox-[0-9a-f]{32}')


def sandbox_declared(manifest):
    return SDK in manifest['sources'] or TEST in manifest['sources']


def rust_structure(text):
    """Mask strings and nested comments so fixture literals cannot declare tests."""
    pattern = re.compile(r'//[^\n]*|/\*|(?:br|r)(?P<hashes>\#*)"|b?"|\'(?:\\.|[^\'\\\n])\'')
    chunks, position = [], 0
    while match := pattern.search(text, position):
        chunks.append(text[position:match.start()])
        token = match.group()
        end = match.end()
        if token == '/*':
            depth = 1
            while depth:
                nested = re.search(r'/\*|\*/', text[end:])
                if not nested:
                    raise ValueError('unterminated Rust comment')
                depth += 1 if nested.group() == '/*' else -1
                end += nested.end()
        elif token.startswith('//') or token.startswith("'"):
            pass
        elif match.group('hashes') is not None:
            tail = '"' + match.group('hashes')
            close = text.find(tail, end)
            if close < 0:
                raise ValueError('unterminated Rust raw string')
            end = close + len(tail)
        else:
            close = re.search(r'(?:\\.|[^"\\])*"', text[end:])
            if not close:
                raise ValueError('unterminated Rust string')
            end += close.end()
        chunks.append(re.sub(r'[^\n]', ' ', text[match.start():end]))
        position = end
    chunks.append(text[position:])
    return ''.join(chunks)


def dispatch_plan(root, manifest=None):
    root = Path(root)
    sources = manifest['sources'] if manifest else inventory(root)
    sqlx, docker = [], []
    for name in sorted(sources):
        if not name.endswith('.rs') or not name.startswith(('src/', 'tests/')):
            continue
        structure = rust_structure((root/name).read_text())
        matched = 0
        for m in re.finditer(r'((?:#\s*\[[^\]]*\]\s*)+)(?:pub(?:\([^)]*\))?\s+)?'
                             r'(?:async\s+)?fn\s+(\w+)\s*\(', structure):
            attrs, case = m.groups()
            if not re.search(r'\bignore\b', attrs):
                continue
            matched += 1
            if re.search(r'\bsqlx\s*::\s*test\b', attrs):
                sqlx.append(case)
            elif name == TEST and case in CASES and re.search(r'#\s*\[\s*test\s*\]', attrs):
                docker.append(case)
            else:
                raise ValueError('unclassified ignored Rust test: ' + name + '::' + case)
        if len(re.findall(r'#\s*\[\s*ignore\b', structure)) != matched:
            raise ValueError('unsupported ignored test attributes: ' + name)
    declared = sandbox_declared({'sources':sources})
    if declared:
        if SDK not in sources or TEST not in sources or set(docker) != CASES or len(docker) != 9:
            raise ValueError('declared sandbox requires SDK and all9 actual tests')
        text = (root/TEST).read_text()
        match = re.search(r'^const\s+IMAGE\s*:\s*&str\s*=\s*"([^"]+)"\s*;', text, re.M)
        if not match or match.group(1) != IMAGE:
            raise ValueError('sandbox test image digest is not the admitted pin')
    elif docker:
        raise ValueError('sandbox tests lack their declared SDK')
    if any(skip in case for skip in docker for case in sqlx):
        raise ValueError('Docker skip filter collides with SQLx case')
    return {'sqlx_cases':sqlx, 'docker_cases':sorted(docker), 'sandbox_declared':declared,
            'image_digest':IMAGE if declared else None}


def database_command(plan):
    command = ['cargo','test','--locked','--workspace','--lib','--tests','--','--ignored']
    for case in plan['docker_cases']:
        command.extend(['--skip',case])
    return command


def database_output_passed(plan, text):
    results = re.findall(r'^test ([\w:]+) \.\.\. (ok|FAILED|ignored(?:,.*)?)$', text, re.M)
    actual = Counter(name.rsplit('::',1)[-1] for name,status in results if status == 'ok')
    return (actual == Counter(plan['sqlx_cases']) and all(status == 'ok' for _,status in results)
            and not any(name.rsplit('::',1)[-1] in CASES for name,_ in results))


def docker_output_passed(text):
    cases = re.findall(r'^test ([\w:]+) \.\.\. ok$', text, re.M)
    rows = re.findall(r'test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored; '
                      r'(\d+) measured; (\d+) filtered out', text)
    return (len(cases) == 9 and set(cases) == CASES and len(rows) == 1 and
            tuple(map(int,rows[0][:4])) == (9,0,0,0) and
            bool(re.search(r'Running tests/sandbox\.rs \(', text)))


def validate_outcomes(directory):
    directory = Path(directory)
    names = {p.name for p in directory.iterdir()}
    if names != {name+'.json' for name in (*OUTCOMES,'foreign_cleanup')}:
        raise ValueError('sandbox outcome inventory differs from required13')
    if any(p.is_symlink() or not p.is_file() or p.stat().st_size > 512*1024
           for p in directory.iterdir()):
        raise ValueError('invalid sandbox outcome file')
    records = {}
    for name,status in OUTCOMES.items():
        p = directory/(name+'.json')
        if p.is_symlink() or p.stat().st_size > 512*1024:
            raise ValueError('invalid sandbox outcome file')
        value = read_json(p)
        required = {'execution_id','status','exit_code','stdout','stderr','output_truncated','duration_ms',
                    'image_digest','isolation_verified','cleanup_passed'}
        if (not isinstance(value,dict) or set(value) != required or
                not isinstance(value['execution_id'],str) or not NAME.fullmatch(value['execution_id']) or
                value['image_digest'] != (('python@sha256:'+'0'*64) if name=='unavailable_image' else IMAGE) or
                value['status'] != status or value['cleanup_passed'] is not True or
                value['isolation_verified'] is not (status != 'unavailable') or
                type(value['duration_ms']) is not int or not 0 <= value['duration_ms'] <= 10000 or
                type(value['output_truncated']) is not bool or
                any(not isinstance(value[k],str) or len(value[k].encode()) > 65536 for k in ('stdout','stderr')) or
                (value['exit_code'] is not None and type(value['exit_code']) is not int)):
            raise ValueError('sandbox outcome contract failed: '+name)
        if status=='succeeded' and value['exit_code'] != 0:
            raise ValueError('sandbox success lacks real exit0')
        if status=='code_failed' and value['exit_code'] != 7:
            raise ValueError('code failure lacks expected real exit7')
        if status=='resource_exceeded' and value['exit_code'] != 137:
            raise ValueError('resource failure lacks real OOM exit137')
        if status=='unavailable' and (value['exit_code'] is not None or value['stdout'] or
                value['stderr'] != ('pinned sandbox image unavailable' if name=='unavailable_image'
                                    else 'sandbox engine unavailable')):
            raise ValueError('unavailable case executed source')
        if status=='output_limit' and value['output_truncated'] is not True:
            raise ValueError('output limit lacks truncation evidence')
        records[name] = value
    foreign = read_json(directory/'foreign_cleanup.json')
    if (not isinstance(foreign,dict) or set(foreign) != {'actual_id','execution_id',
            'mismatched_owner_refused','foreign_fixture_preserved_before_owned_teardown','manual_fixture_cleanup'} or
            not isinstance(foreign.get('execution_id'),str) or
            not NAME.fullmatch(foreign['execution_id']) or not isinstance(foreign.get('actual_id'),str) or
            not re.fullmatch('[0-9a-f]{64}',foreign['actual_id']) or
            not all(foreign.get(k) is True for k in ('mismatched_owner_refused',
                'foreign_fixture_preserved_before_owned_teardown','manual_fixture_cleanup'))):
        raise ValueError('foreign ownership control failed')
    records['foreign_cleanup'] = foreign
    if len({v['execution_id'] for v in records.values()}) != 13:
        raise ValueError('sandbox executions reused handles')
    return records


def stage_report_passed(output, manifest, checkpoint_sha):
    """Exit0 cannot replace the source-bound child evidence protocol."""
    try:
        output = Path(output)
        value = read_json(output/'parent-report.json')
        return (value.get('format') == 'alia-p03-sandbox-tests/1' and
            value.get('domain') == 'actual-Docker-not-SQLx' and
            all(value.get(k) is True for k in ('passed','final_integrity','private_runtime_cleaned')) and
            type(value.get('model_calls')) is int and value['model_calls'] == 0 and
            value.get('image_digest') == IMAGE and value.get('checkpoint_sha256') == checkpoint_sha and
            value.get('sources') == {p:manifest['sources'][p] for p in (SDK,TEST)} and
            value.get('expected_cases') == sorted(CASES) and value.get('child_exit') == 0 and
            value.get('actual_tests') == 9 and value.get('outcome_records') == 13 and
            bool(value.get('cleanup')) and all(v.get('passed') is True for v in value['cleanup']) and
            docker_output_passed((output/'cargo.log').read_text()) and
            len(validate_outcomes(output/'outcomes')) == 13)
    except (OSError,ValueError,KeyError,TypeError,AttributeError):
        return False


def cleanup_resources(resources, docker):
    """Only exact identities created after empty-namespace locked admission."""
    records = []
    leftovers = resources()
    for value in leftovers:
        name = value['name'].removeprefix('/')
        if (NAME.fullmatch(name) and value['labels'].get('io.alia.sandbox.execution')==name
                and value['labels'].get('io.alia.sandbox.role')=='oneshot' and value['image']==IMAGE):
            # Re-inspect immediately before deletion; do not trust a stale listing.
            try:
                fresh = next((v for v in resources() if v['id']==value['id']),None)
            except Exception as error:
                records.append({'passed':False,'id':value['id'],'name':name,
                                'reason':'identity recheck failed; retained','error':type(error).__name__})
                continue
            if fresh != value:
                records.append({'passed':False,'id':value['id'],'reason':'identity changed; retained'})
                continue
            try:
                docker(['container','rm','--force',value['id']])
                records.append({'passed':True,'recovered_id':value['id'],'name':name})
            except Exception as error:
                records.append({'passed':False,'id':value['id'],'name':name,
                                'reason':'owned removal failed; retained','error':type(error).__name__})
        else:
            records.append({'passed':False,'id':value['id'],'name':name,
                            'reason':'identity not verified; retained'})
    try:
        remaining = resources()
        records.append({'passed':not remaining,'remaining':remaining})
    except Exception as error:
        records.append({'passed':False,'reason':'final identity query failed','error':type(error).__name__})
    return records, bool(leftovers)


def child_environment(target, output, locks):
    env = clean_env()
    for key in ('CARGO_BUILD_JOBS','CARGO_PROFILE_DEV_DEBUG','CARGO_PROFILE_TEST_DEBUG'):
        if key in os.environ:
            env[key] = os.environ[key]
    env.update(CARGO_TARGET_DIR=str(target), CARGO_INCREMENTAL='0',RUST_TEST_THREADS='1',
        ALIA_SANDBOX_ACTUAL='1',ALIA_SANDBOX_REPORT_DIR=str(output),
        ALIA_SANDBOX_PRIVATE_SENTINEL='SYNTHETIC_PARENT_SECRET',
        HTTP_PROXY='http://127.0.0.1:1',DOCKER_HOST='tcp://127.0.0.1:1')
    if isinstance(locks, int):  # compatibility for direct environment unit fixtures
        env['ALIA_QA_LOCK_FD'] = str(locks)
        return env
    return locks.child_environment(env)


def run_stage(args):
    for name in ('checkpoint','source_root','target_dir','output'):
        setattr(args,name,getattr(args,name).resolve())
    if any(p==args.source_root or args.source_root in p.parents or p==args.checkpoint or
           args.checkpoint in p.parents for p in (args.target_dir,args.output)):
        raise ValueError('sandbox target/output must be outside immutable inputs')
    args.output.mkdir(parents=True)
    report = {'format':'alia-p03-sandbox-tests/1','started_at':utc(),'passed':False,
              'cleanup':[],'model_calls':0,'image_digest':IMAGE,'domain':'actual-Docker-not-SQLx'}
    with resource_locks(('sandbox',str(args.output)),('target',str(args.target_dir))) as locks:
        report['queue_seconds'] = locks.queue_seconds
        private = Path(tempfile.mkdtemp(prefix='alia-p03-sandbox-qa-'))
        deadline = time.monotonic()+220
        try:
            (private/'outcomes').mkdir(mode=0o700)
            (private/'config.json').write_text('{}');(private/'config.json').chmod(0o600)
            command = DOCKER+['--config',str(private),'--host','unix:///var/run/docker.sock']
            def docker(arguments):
                p = subprocess.run(command+arguments,env={'PATH':'/usr/bin:/bin','HOME':str(private)},
                    capture_output=True,text=True,timeout=max(.01,min(15,deadline-time.monotonic())),
                    pass_fds=locks.pass_fds)
                if p.returncode: raise RuntimeError('sandbox QA Docker control failed')
                return p.stdout.strip()
            def resources():
                ids = docker(['container','ls','--all','--quiet','--no-trunc','--filter',
                              'label=io.alia.sandbox.role=oneshot']).splitlines()
                values = []
                for id in ids:
                    if not re.fullmatch('[0-9a-f]{64}',id): raise ValueError('invalid container identity')
                    value = json.loads(docker(['container','inspect',id,'--format',
                        '{"id":{{json .Id}},"name":{{json .Name}},"image":{{json .Config.Image}},"labels":{{json .Config.Labels}}}']))
                    if (not isinstance(value,dict) or value.get('id') != id or
                            not isinstance(value.get('name'),str) or not isinstance(value.get('labels'),dict) or
                            not isinstance(value.get('image'),str)):
                        raise ValueError('container inspection identity mismatch')
                    value['labels'] = {k:value['labels'].get(k) for k in
                                      ('io.alia.sandbox.execution','io.alia.sandbox.role')}
                    values.append(value)
                return values
            admitted = False
            process = None
            try:
                report['phase'] = 'source-admission'
                manifest = verify(args.checkpoint)
                if inventory(args.source_root) != manifest['sources']:
                    raise ValueError('sandbox execution source differs from checkpoint')
                plan = dispatch_plan(args.source_root,manifest)
                if not plan['sandbox_declared']: raise ValueError('sandbox source not declared')
                report.update(checkpoint_sha256=digest(args.checkpoint/'manifest.json'),
                    sources={p:manifest['sources'][p] for p in (SDK,TEST)},expected_cases=plan['docker_cases'])
                report['phase'] = 'cached-image-and-empty-namespace-admission'
                refs = json.loads(docker(['image','inspect',IMAGE,'--format','{{json .RepoDigests}}']))
                if not isinstance(refs,list) or IMAGE not in refs:
                    raise ValueError('pinned sandbox image unavailable')
                if resources(): raise ValueError('sandbox QA namespace already occupied; leave untouched')
                admitted = True
                report['qa_namespace_admitted_empty'] = True
                report['phase'] = 'actual-nine-Docker-tests'
                env = child_environment(args.target_dir,private/'outcomes',locks)
                argv = ['cargo','test','--locked','--test','sandbox','--','--ignored','--nocapture']
                report['command'] = argv
                process = subprocess.Popen(argv,cwd=args.source_root,env=env,pass_fds=locks.pass_fds,
                    stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,start_new_session=True)
                try:
                    stdout,stderr = process.communicate(timeout=max(.01,min(180,deadline-time.monotonic()-35)))
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid,signal.SIGKILL)
                    stdout,stderr = process.communicate(timeout=5)
                    report['timeout'] = True
                text = stdout+'\n'+stderr
                (args.output/'cargo.log').write_text(text)
                report['child_exit'] = process.returncode
                if process.returncode or report.get('timeout') or not docker_output_passed(text):
                    raise ValueError('actual sandbox9 test execution unverified or failed')
                report['phase'] = 'outcomes-and-exact-absence'
                records = validate_outcomes(private/'outcomes')
                for value in records.values():
                    # Successful list query, exact escaped name; daemon failures never mean absence.
                    if docker(['container','ls','--all','--quiet','--no-trunc','--filter',
                               'name=^/'+value['execution_id']+'$']):
                        raise ValueError('reported sandbox cleanup left a container')
                public = args.output/'outcomes';public.mkdir()
                for name,value in records.items():write_new(public/(name+'.json'),value)
                report['actual_tests'] = 9
                report['outcome_records'] = 13
                report['passed'] = True
            except (Exception,KeyboardInterrupt) as error:
                report['failure'] = type(error).__name__+': sandbox QA phase failed'
            finally:
                handles = set()
                for path in (private/'outcomes').glob('*.json'):
                    try:
                        value = read_json(path)
                        handle = value.get('execution_id') if isinstance(value,dict) else None
                        if isinstance(handle,str) and NAME.fullmatch(handle):handles.add(handle)
                    except (OSError,ValueError):pass
                report['reported_execution_handles'] = sorted(handles)
                if process is not None and process.poll() is None:
                    try:os.killpg(process.pid,signal.SIGKILL)
                    except ProcessLookupError:pass
                    except OSError as error:
                        report['cleanup'].append({'passed':False,'launcher_stop_error':type(error).__name__})
                if admitted:
                    try:
                        records,recovered = cleanup_resources(resources,docker)
                        report['cleanup'].extend(records)
                        if recovered:
                            report['passed'] = False
                            report['recovery_required'] = True
                    except Exception as error:
                        report['cleanup'].append({'passed':False,'error':type(error).__name__})
                if process is not None:
                    try:
                        process.communicate(timeout=5)
                        report['cleanup'].append({'passed':process.poll() is not None,'launcher_reaped':True})
                    except Exception as error:
                        report['cleanup'].append({'passed':False,'launcher_reap_error':type(error).__name__})
                try:
                    report['final_integrity'] = (verify(args.checkpoint)==manifest and
                        inventory(args.source_root)==manifest['sources'])
                except Exception:report['final_integrity'] = False
                report['passed'] = (report['passed'] and report['final_integrity'] and
                                    all(v['passed'] for v in report['cleanup']))
        except (Exception,KeyboardInterrupt) as error:
            report.update(passed=False,private_runtime_error=type(error).__name__)
        finally:
            try:
                if all(v['passed'] for v in report['cleanup']):shutil.rmtree(private)
                else:report['private_recovery_directory'] = str(private)
            except OSError as error:report['private_cleanup_error'] = type(error).__name__
        report['private_runtime_cleaned'] = not private.exists()
        if not report['private_runtime_cleaned']:
            report['private_recovery_directory'] = str(private)
        report['passed'] &= report['private_runtime_cleaned']
        report['ended_at'] = utc()
        write_new(args.output/'parent-report.json',report)
    print(json.dumps({'passed':report['passed'],'report':str(args.output/'parent-report.json')}))
    return 0 if report['passed'] else 1


def main():
    p=argparse.ArgumentParser(description=__doc__)
    for name in ('checkpoint','source-root','target-dir','output'):
        p.add_argument('--'+name,type=Path,required=True)
    args=p.parse_args()
    def interrupted(signum,frame):
        raise InterruptedError('sandbox QA interrupted')
    signal.signal(signal.SIGTERM,interrupted)
    return run_stage(args)

if __name__=='__main__':raise SystemExit(main())
