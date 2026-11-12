//! Explicit isolated tests only. The production adapter is deliberately unregistered.
mod domain {
    pub use alia::domain::config;
}
#[path = "../src/adapters/sandbox.rs"]
pub mod sandbox;
use sandbox::{DockerCommand, SandboxExecutor, SandboxOutcome, SandboxRequest, SandboxStatus};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream, UdpSocket},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

const IMAGE: &str =
    "python@sha256:a5a7017efc5969e00ae2b91c8eb0631049a569652aa4ffc4501769049ae45c72";

#[test]
#[ignore = "explicit owned Docker sandbox tests"]
fn actual_foreign_identity_is_not_removed_by_owned_cleanup() {
    let value = executor().foreign_cleanup_probe();
    if let Ok(directory) = std::env::var("ALIA_SANDBOX_REPORT_DIR") {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(std::path::Path::new(&directory).join("foreign_cleanup.json"))
            .unwrap();
        f.write_all(&serde_json::to_vec_pretty(&value).unwrap())
            .unwrap();
    }
    assert_eq!(value["mismatched_owner_refused"], true);
    assert_eq!(
        value["foreign_fixture_preserved_before_owned_teardown"],
        true
    );
    assert_eq!(value["manual_fixture_cleanup"], true);
}

#[test]
#[ignore = "explicit owned Docker sandbox tests"]
fn unavailable_native_launcher_never_escalates_to_sudo_or_executes_source() {
    let _ = executor();
    let native = SandboxExecutor::new(IMAGE, DockerCommand::Native).unwrap();
    let out = native.execute(&SandboxRequest {
        source: "print('NATIVE_MUST_NOT_RUN')".into(),
        timeout_ms: 10000,
    });
    record("native_unavailable", &out);
    // This QA account actually lacks native Docker socket permission. The
    // SudoNoPassword positive controls above do not authorize automatic fallback.
    assert_eq!(out.status, SandboxStatus::Unavailable, "{out:?}");
    assert!(out.cleanup_passed && !out.isolation_verified && out.stdout.is_empty());
}

fn executor() -> SandboxExecutor {
    assert_eq!(
        std::env::var("ALIA_SANDBOX_ACTUAL").as_deref(),
        Ok("1"),
        "explicit isolated test admission required"
    );
    let fd = std::env::var("ALIA_QA_LOCK_FD").expect("parent global lock fd required");
    let inherited = std::fs::metadata(format!("/proc/self/fd/{fd}")).unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/tmp/alia-project-heavy.lock")
        .unwrap();
    assert_eq!(inherited.ino(), lock.metadata().unwrap().ino());
    assert!(
        matches!(lock.try_lock(), Err(std::fs::TryLockError::WouldBlock)),
        "actual parent heavy lock must remain held"
    );
    SandboxExecutor::new(IMAGE, DockerCommand::SudoNoPassword).unwrap()
}
fn record(name: &str, outcome: &SandboxOutcome) {
    if let Ok(directory) = std::env::var("ALIA_SANDBOX_REPORT_DIR") {
        let p = std::path::Path::new(&directory).join(format!("{name}.json"));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(p)
            .unwrap();
        file.write_all(&serde_json::to_vec_pretty(outcome).unwrap())
            .unwrap();
    }
}
fn execute(name: &str, source: String, timeout_ms: u64) -> SandboxOutcome {
    let started = Instant::now();
    let out = executor().execute(&SandboxRequest { source, timeout_ms });
    record(name, &out);
    assert!(
        started.elapsed() < Duration::from_millis(timeout_ms + 250),
        "whole wall bound including cleanup"
    );
    assert!(out.cleanup_passed, "{out:?}");
    assert!(out.stdout.len() <= 65536 && out.stderr.len() <= 65536);
    out
}

#[test]
#[ignore = "explicit owned Docker sandbox tests; admitted parent global lock and pinned image required"]
fn normal_computation_stdin_and_clean_environment_are_actual() {
    let source=r#"import os,sys,json
status=dict(line.split(':',1) for line in open('/proc/self/status') if ':' in line)
mounts=[line.split() for line in open('/proc/self/mountinfo')]
print(json.dumps({'sum':sum(range(1000)), 'argv':open('/proc/self/cmdline','rb').read().decode().split(chr(0))[:-1], 'env':dict(os.environ), 'uid':os.getuid(),'gid':os.getgid(),'cwd':os.getcwd(), 'controls':{'seccomp':status['Seccomp'].strip(),'no_new_privs':status['NoNewPrivs'].strip(),'cap_eff':status['CapEff'].strip(),'cgroup':{name:open('/sys/fs/cgroup/'+name).read().strip() for name in ['memory.max','memory.swap.max','pids.max','cpu.max']},'mounts':{path:{'options':next(m for m in mounts if m[4]==path)[5],'mode':oct(os.stat(path).st_mode & 0o7777),**({'tmpfs_options':next(m for m in mounts if m[4]==path)[-1]} if path!='/' else {})} for path in ['/','/tmp','/workspace']},'interfaces':[line.split(':')[0].strip() for line in open('/proc/net/dev') if ':' in line]}}))
"#.into();
    let out = execute("normal", source, 10000);
    assert_eq!(out.status, SandboxStatus::Succeeded, "{out:?}");
    assert!(out.isolation_verified);
    assert_eq!(out.exit_code, Some(0));
    let data: serde_json::Value = serde_json::from_str(out.stdout.trim()).unwrap();
    assert_eq!(data["sum"], 499500);
    assert_eq!(
        data["argv"],
        serde_json::json!(["/usr/local/bin/python", "-I", "-B", "-"])
    );
    assert_eq!(
        data["env"],
        serde_json::json!({"PATH":"/usr/local/bin:/usr/bin:/bin"})
    );
    assert_eq!(data["uid"], 65534);
    assert_eq!(data["gid"], 65534);
    assert_eq!(data["controls"]["seccomp"], "2");
    assert_eq!(data["controls"]["no_new_privs"], "1");
    assert_eq!(data["controls"]["cgroup"]["memory.max"], "134217728");
    assert_eq!(data["controls"]["cgroup"]["memory.swap.max"], "0");
    assert_eq!(data["controls"]["cgroup"]["pids.max"], "64");
    assert_eq!(data["controls"]["interfaces"], serde_json::json!(["lo"]));
    assert!(!format!("{out:?}").contains("SYNTHETIC_PARENT_SECRET"));
}

#[test]
#[ignore = "explicit owned Docker sandbox tests"]
fn maximum_utf8_source_uses_stdin_and_real_nonzero_exit_is_code_failed() {
    let mut source = "print(42)\n#".to_owned();
    source.push_str(&"x".repeat(32768 - source.len()));
    let out = execute("maximum_source", source, 10000);
    assert_eq!(out.status, SandboxStatus::Succeeded, "{out:?}");
    assert_eq!(out.stdout.trim(), "42");
    let out = execute(
        "code_failed",
        "import sys; sys.stderr.write('expected-code-error'); sys.exit(7)".into(),
        10000,
    );
    assert_eq!(out.status, SandboxStatus::CodeFailed, "{out:?}");
    assert_eq!(out.exit_code, Some(7));
    assert!(out.isolation_verified);
    assert_eq!(out.stderr, "expected-code-error");
}

#[test]
#[ignore = "explicit owned Docker sandbox tests"]
fn filesystem_tmpfs_noexec_and_no_host_data_or_socket_are_actual() {
    let directory = std::env::temp_dir().join(format!(
        "alia-sandbox-host-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .unwrap();
    let sentinel = directory.join("sentinel");
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&sentinel)
        .unwrap()
        .write_all(b"SYNTHETIC_HOST_PRIVATE_FILE")
        .unwrap();
    let source = format!(
        r#"import os,errno,json,subprocess
assert not os.path.exists({sentinel:?})
for path in ['/var/run/docker.sock','/workapps/project-alia','/home/zero/Documents/Codex']:
    assert not os.path.exists(path)
for path in ['/etc/alia-sandbox-write','/root/alia-sandbox-write']:
    try: open(path,'w').write('forbidden')
    except OSError as e: assert e.errno in (errno.EROFS,errno.EACCES)
    else: raise AssertionError('rootfs write permitted')
open('/workspace/private','w').write('workspace')
open('/tmp/private','w').write('tmp')
assert open('/workspace/private').read()=='workspace'
try: open('/workspace/overflow','wb').write(b'x'*(20*1024*1024))
except OSError as e: assert e.errno==errno.ENOSPC
else: raise AssertionError('tmpfs size not enforced')
open('/tmp/noexec','w').write('#!/bin/sh\nexit 0\n');os.chmod('/tmp/noexec',0o755)
try: subprocess.run(['/tmp/noexec'])
except PermissionError: pass
else: raise AssertionError('tmpfs execution allowed')
print('filesystem-boundaries-verified')
"#,
        sentinel = sentinel.to_string_lossy()
    );
    let out = execute("filesystem", source, 10000);
    let retained = std::fs::read_to_string(&sentinel).unwrap();
    std::fs::remove_dir_all(directory).unwrap();
    assert_eq!(retained, "SYNTHETIC_HOST_PRIVATE_FILE");
    assert_eq!(out.status, SandboxStatus::Succeeded, "{out:?}");
    assert!(out.isolation_verified);
}

#[test]
#[ignore = "explicit owned Docker sandbox tests"]
fn network_tcp_and_dns_have_real_host_positive_controls_and_cannot_reach_them() {
    let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
    let tcp_port = tcp.local_addr().unwrap().port();
    tcp.set_nonblocking(true).unwrap();
    let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
    let dns_port = udp.local_addr().unwrap().port();
    udp.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    let server = std::thread::spawn(move || {
        while !flag.load(Ordering::Relaxed) {
            if let Ok((mut stream, _)) = tcp.accept() {
                let _ = stream.write_all(b"synthetic-host-tcp");
            }
            let mut bytes = [0; 512];
            if let Ok((n, address)) = udp.recv_from(&mut bytes)
                && n >= 12
            {
                let mut answer = bytes[..n].to_vec();
                answer[2] = 0x81;
                answer[3] = 0x80;
                answer[7] = 1;
                answer.extend_from_slice(&[0xc0, 0x0c, 0, 1, 0, 1, 0, 0, 0, 1, 0, 4, 127, 0, 0, 1]);
                let _ = udp.send_to(&answer, address);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    });
    let positive = || {
        let mut stream = TcpStream::connect(("127.0.0.1", tcp_port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(500)))
            .unwrap();
        let mut text = String::new();
        stream.read_to_string(&mut text).unwrap();
        assert_eq!(text, "synthetic-host-tcp");
        let client = UdpSocket::bind("127.0.0.1:0").unwrap();
        client
            .set_read_timeout(Some(Duration::from_millis(500)))
            .unwrap();
        client.send_to(b"\x12\x34\x01\x00\x00\x01\x00\x00\x00\x00\x00\x00\x07sandbox\x04test\x00\x00\x01\x00\x01",("127.0.0.1",dns_port)).unwrap();
        let mut answer = [0; 512];
        let n = client.recv(&mut answer).unwrap();
        assert_eq!(&answer[n - 4..n], &[127, 0, 0, 1]);
    };
    positive();
    let source = format!(
        r#"import socket,errno
try: socket.create_connection(('127.0.0.1',{tcp_port}),timeout=.2)
except OSError: pass
else: raise AssertionError('host TCP reached')
s=socket.socket(socket.AF_INET,socket.SOCK_DGRAM);s.settimeout(.2)
s.sendto(bytes.fromhex('1234010000010000000000000773616e64626f7804746573740000010001'),('127.0.0.1',{dns_port}))
try: s.recv(512)
except OSError: pass
else: raise AssertionError('host DNS reached')
try: s.connect(('192.0.2.1',53))
except OSError as e: assert e.errno in (errno.ENETUNREACH,errno.EHOSTUNREACH)
else: raise AssertionError('external route exists')
print('tcp-dns-network-none-verified')
"#
    );
    let out = execute("network", source, 10000);
    positive();
    stop.store(true, Ordering::Relaxed);
    server.join().unwrap();
    assert_eq!(out.status, SandboxStatus::Succeeded, "{out:?}");
    assert!(out.isolation_verified);
}

#[test]
#[ignore = "explicit owned Docker sandbox tests"]
fn real_cgroups_bound_processes_memory_swap_and_cpu() {
    let source=r#"import os,time,errno,json
assert open('/sys/fs/cgroup/pids.max').read().strip()=='64'
assert open('/sys/fs/cgroup/memory.max').read().strip()=='134217728'
assert open('/sys/fs/cgroup/memory.swap.max').read().strip()=='0'
quota,period=open('/sys/fs/cgroup/cpu.max').read().split();assert int(quota)==int(period)
children=[];blocked=False
for i in range(100):
    try: pid=os.fork()
    except OSError as e: assert e.errno==errno.EAGAIN;blocked=True;break
    if pid==0: time.sleep(.5);os._exit(0)
    children.append(pid)
for pid in children: os.waitpid(pid,0)
assert blocked and 0<len(children)<64
events=dict(line.split() for line in open('/sys/fs/cgroup/pids.events'));assert int(events['max'])>0
print(json.dumps({'created_children':len(children),'pid_limit_rejection':blocked,'pids_events_max':events['max'],'cpu_max':[quota,period]}))
"#.into();
    let out = execute("cgroup_process_cpu", source, 10000);
    assert_eq!(out.status, SandboxStatus::Succeeded, "{out:?}");
    assert!(out.isolation_verified);
    let out = execute(
        "memory_oom",
        "x=bytearray(256*1024*1024); print(len(x))".into(),
        10000,
    );
    assert_eq!(out.status, SandboxStatus::ResourceExceeded, "{out:?}");
    assert!(out.isolation_verified);
    assert_eq!(out.exit_code, Some(137));
}

#[test]
#[ignore = "explicit owned Docker sandbox tests"]
fn infinite_time_and_output_are_actually_stopped_with_bounded_evidence() {
    let out = execute("timed_out", "while True: pass".into(), 3000);
    assert_eq!(out.status, SandboxStatus::TimedOut, "{out:?}");
    assert!(out.isolation_verified);
    assert!(out.duration_ms <= 3250);
    let out = execute(
        "output_limit",
        "import os\nwhile True:\n os.write(1,b'x'*4096)\n os.write(2,b'y'*4096)".into(),
        10000,
    );
    assert_eq!(out.status, SandboxStatus::OutputLimit, "{out:?}");
    assert!(out.isolation_verified);
    assert!(out.output_truncated);
    let out = execute(
        "invalid_utf8_output",
        "import os; os.write(1,b'\\xff'*65536)".into(),
        10000,
    );
    assert_eq!(out.status, SandboxStatus::OutputLimit, "{out:?}");
    assert!(out.isolation_verified && out.output_truncated);
}

#[test]
#[ignore = "explicit owned Docker sandbox tests"]
fn missing_image_is_unavailable_without_pull_or_execution_fallback() {
    let _ = executor();
    let s = SandboxExecutor::new(
        &format!("python@sha256:{}", "0".repeat(64)),
        DockerCommand::SudoNoPassword,
    )
    .unwrap();
    let out = s.execute(&SandboxRequest {
        source: "print('MUST_NOT_EXECUTE')".into(),
        timeout_ms: 10000,
    });
    record("unavailable_image", &out);
    assert_eq!(out.status, SandboxStatus::Unavailable, "{out:?}");
    assert!(out.cleanup_passed);
    assert!(!out.isolation_verified);
    assert!(out.stdout.is_empty());
    assert!(!out.stderr.contains("MUST_NOT_EXECUTE"));
}
