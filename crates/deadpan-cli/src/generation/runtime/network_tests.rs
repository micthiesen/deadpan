//! Real launch checks through the model smoke runner and inference host.

use super::BridgeRuntime;
use std::fs;
use std::net::{TcpListener, TcpStream, UdpSocket};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

pub(crate) struct Probe {
    directory: tempfile::TempDir,
    tcp: TcpListener,
    udp: UdpSocket,
    unix: UnixListener,
}

impl Probe {
    pub(crate) fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let tcp = TcpListener::bind("127.0.0.1:0").unwrap();
        let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
        let unix_path = directory.path().join("network.sock");
        let unix = UnixListener::bind(&unix_path).unwrap();
        // Positive controls: these destinations are reachable before the
        // worker starts. Drain them so any later packet is a test failure.
        let _connected = TcpStream::connect(tcp.local_addr().unwrap()).unwrap();
        tcp.accept().unwrap();
        let sender = UdpSocket::bind("127.0.0.1:0").unwrap();
        sender
            .send_to(b"control", udp.local_addr().unwrap())
            .unwrap();
        udp.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut bytes = [0; 32];
        assert_eq!(udp.recv(&mut bytes).unwrap(), 7);
        let _connected = UnixStream::connect(&unix_path).unwrap();
        unix.accept().unwrap();
        tcp.set_nonblocking(true).unwrap();
        udp.set_nonblocking(true).unwrap();
        unix.set_nonblocking(true).unwrap();
        fs::write(
            directory.path().join("probe.json"),
            serde_json::to_vec(&serde_json::json!({
                "tcp": tcp.local_addr().unwrap().port(),
                "udp": udp.local_addr().unwrap().port(),
                "unix": unix_path,
                "root": directory.path(),
            }))
            .unwrap(),
        )
        .unwrap();
        fs::write(directory.path().join("worker.py"), WORKER).unwrap();
        Self {
            directory,
            tcp,
            udp,
            unix,
        }
    }

    pub(crate) fn runtime(&self) -> BridgeRuntime {
        let python = std::env::split_paths(&std::env::var_os("PATH").expect("test PATH"))
            .map(|directory| directory.join("python3"))
            .find(|path| path.is_file())
            .expect("Python 3 is required for AI network isolation tests");
        BridgeRuntime {
            python: python.canonicalize().unwrap(),
            worker_script: self.directory.path().join("worker.py"),
            runtime_source: self.directory.path().to_owned(),
            model_cache: self.directory.path().to_owned(),
            model_manifest: deadpan_models::packs::approved_pack("ltx-2.3-q4-bridge").unwrap(),
            ffmpeg: PathBuf::from("/usr/bin/true"),
            ffprobe: PathBuf::from("/usr/bin/true"),
            media_worker: PathBuf::from("/usr/bin/true"),
            landmark_worker: PathBuf::from("/usr/bin/true"),
        }
    }

    pub(crate) fn assert_denied(&self) -> serde_json::Value {
        let report: serde_json::Value = serde_json::from_slice(
            &fs::read(self.directory.path().join("report.json"))
                .expect("worker wrote local report"),
        )
        .unwrap();
        let parent = &report["parent"];
        assert_eq!(parent["parent_pid"], std::process::id());
        assert_eq!(
            parent["pid"], parent["group"],
            "Python must replace the group leader"
        );
        for name in ["parent", "child"] {
            let part = &report[name];
            assert_eq!(part["file_roundtrip"], true);
            for transport in ["tcp", "udp", "unix"] {
                let error = part[transport].as_i64().expect("socket failure errno");
                assert!([1, 13].contains(&error), "{name} {transport}: {report}");
            }
            let raw_pid = i32::try_from(part["pid"].as_i64().unwrap()).unwrap();
            let pid = rustix::process::Pid::from_raw(raw_pid).unwrap();
            assert_eq!(
                rustix::process::test_kill_process(pid),
                Err(rustix::io::Errno::SRCH),
                "worker was not reaped"
            );
        }
        // The child deliberately changed session/group. The network policy
        // still follows it; the Python parent owns and reaps that child.
        assert_ne!(report["parent"]["group"], report["child"]["group"]);
        assert_eq!(report["child"]["pid"], report["child"]["group"]);
        assert_eq!(
            self.tcp.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        assert_eq!(
            self.unix.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        assert_eq!(
            self.udp.recv(&mut [0; 32]).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        report
    }
}

#[test]
fn ai_network_smoke_check_denies_sockets_and_descendants_while_files_work() {
    for (pack, operation) in [
        (super::BRIDGE_PACK, "bridge_hold"),
        (super::EXTENSION_PACK, "extension_hold"),
    ] {
        let probe = Probe::new();
        let mut runtime = probe.runtime();
        runtime.model_manifest = deadpan_models::packs::approved_pack(pack).unwrap();
        let report = runtime.check(&AtomicBool::new(false)).unwrap();
        assert_eq!(report["operation"], operation);
        probe.assert_denied();
    }
}

const WORKER: &str = r#"
import argparse, json, os, socket, struct, subprocess, sys
from pathlib import Path

def observe(config):
    result = {'pid': os.getpid(), 'parent_pid': os.getppid(), 'group': os.getpgrp()}
    for name, family, kind, address in [
        ('tcp', socket.AF_INET, socket.SOCK_STREAM, ('127.0.0.1', config['tcp'])),
        ('udp', socket.AF_INET, socket.SOCK_DGRAM, ('127.0.0.1', config['udp'])),
        ('unix', socket.AF_UNIX, socket.SOCK_STREAM, config['unix']),
    ]:
        try:
            with socket.socket(family, kind) as connection:
                connection.settimeout(2)
                if name == 'udp':
                    connection.sendto(b'forbidden', address)
                else:
                    connection.connect(address)
        except OSError as error:
            result[name] = error.errno
        else:
            result[name] = 0
    path = Path(config['root']) / ('local-' + str(os.getpid()))
    path.write_bytes(b'local files remain available')
    result['file_roundtrip'] = path.read_bytes() == b'local files remain available'
    return result

if len(sys.argv) > 1 and sys.argv[1] == '--descendant':
    print(json.dumps(observe(json.loads(Path(sys.argv[2]).read_text()))))
    sys.exit(0)

parser = argparse.ArgumentParser()
parser.add_argument('--runtime-config', required=True)
parser.add_argument('--check', action='store_true')
args = parser.parse_args()
runtime = json.loads(Path(args.runtime_config).read_text())
config_path = Path(runtime['runtime_source']) / 'probe.json'
config = json.loads(config_path.read_text())
child = subprocess.run([sys.executable, '-I', '-B', __file__, '--descendant', str(config_path)],
                       start_new_session=True, capture_output=True, timeout=10, check=True)
report = {'parent': observe(config), 'child': json.loads(child.stdout)}
(Path(config['root']) / 'report.json').write_text(json.dumps(report))
if args.check:
    pack = runtime['model_pack']
    print(json.dumps({'schema_version': 2, 'runtime_commit': '3392d75934120b7e69eefbe55893f7ef82be92a4',
        'operation': pack['operations'][0], 'pack_id': pack['pack_id'], 'pack_version': pack['pack_version'],
        'runtime_id': pack['runtime_id'], 'runtime_version': pack['runtime_versions'][0],
        'model_manifest_sha256': runtime['model_manifest_sha256'], 'device': 'Device(gpu, 0)',
        'adapter_sources_sha256': {name: 'a' * 64 for name in ['worker.py', 'worker_protocol.py',
            'worker_media.py', 'worker_extension_context.py', 'mlx_backend.py', 'runtime_source.py',
            'ltx-source-manifest.json']},
        'python': sys.version.split()[0], 'mlx': 'probe', 'verified_assets': len(pack['files']),
        'safetensors_tensors': 1, 'loaded_ltx_sources': 1, 'seconds': 0}))
else:
    def exact(length):
        result = bytearray()
        while len(result) < length:
            chunk = sys.stdin.buffer.read(length - len(result))
            if not chunk:
                raise RuntimeError('host request ended early')
            result.extend(chunk)
        return bytes(result)
    length = struct.unpack('>I', exact(4))[0]
    assert 0 < length <= 256 * 1024
    request = json.loads(exact(length))
    assert request['operation'] == 'generate_bridge'
    def emit(message):
        data = json.dumps(message).encode('utf-8')
        sys.stdout.buffer.write(struct.pack('>I', len(data)) + data)
        sys.stdout.buffer.flush()
    emit({'event': 'stage', 'protocol': 2, 'identity': request['identity'], 'stage': 'preflight'})
    emit({'event': 'failed', 'protocol': 2, 'identity': request['identity'],
          'failure': {'code': 'backend_failure', 'detail': 'network isolation probe finished'}})
"#;
