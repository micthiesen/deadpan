use std::io::{Read, Write};
use std::net::Shutdown;
use std::sync::atomic::AtomicBool;

use super::*;

fn pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let server = listener.accept().unwrap().0;
    for stream in [&client, &server] {
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
    }
    (client, server)
}

#[test]
fn relay_preserves_bytes_and_half_closed_replies_across_many_buffers() {
    let (mut client, local) = pair();
    let (remote, mut server) = pair();
    let expected: Vec<u8> = (0..300_000u32).flat_map(u32::to_le_bytes).collect();
    let expected_server = expected.clone();
    let server_thread = std::thread::spawn(move || {
        let mut received = Vec::new();
        server.read_to_end(&mut received).unwrap();
        assert_eq!(received, expected_server);
        server.write_all(&received).unwrap();
        server.write_all(b"finished").unwrap();
    });
    let relay = std::thread::spawn(move || tunnel::relay(local, remote, &AtomicBool::new(false)));
    client.write_all(&expected).unwrap();
    client.shutdown(Shutdown::Write).unwrap();
    let mut received = Vec::new();
    client.read_to_end(&mut received).unwrap();
    assert_eq!(&received[..expected.len()], expected);
    assert_eq!(&received[expected.len()..], b"finished");
    server_thread.join().unwrap();
    relay.join().unwrap().unwrap();
}

#[test]
fn cancellation_ends_an_idle_tunnel_without_waiting_for_its_peer() {
    let (_client, local) = pair();
    let (remote, _server) = pair();
    let stopped = Arc::new(AtomicBool::new(false));
    let stop = Arc::clone(&stopped);
    let relay = std::thread::spawn(move || tunnel::relay(local, remote, &stop));
    let started = Instant::now();
    stopped.store(true, Ordering::Release);
    relay.join().unwrap().unwrap();
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn an_approved_hostname_resolving_to_private_addresses_never_connects() {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    // The actual port is deliberately preserved: neither a forbidden port nor
    // a private address may reach the test service after DNS admission.
    let private = listener.local_addr().unwrap();
    let (mut client, local) = pair();
    let credentials = protocol::Credentials::new();
    let request = format!(
        "CONNECT www.youtube.com:443 HTTP/1.1\r\nProxy-Authorization: {}\r\n\r\n",
        credentials.authorization
    );
    let restricted = Arc::new(AtomicBool::new(false));
    let refused = Arc::clone(&restricted);
    let gateway = std::thread::spawn(move || {
        serve_with_resolver(
            local,
            &AtomicBool::new(false),
            &refused,
            &credentials,
            |host| {
                assert_eq!(host, "www.youtube.com");
                Ok(vec![
                    private,
                    "127.0.0.1:443".parse().unwrap(),
                    "[::1]:443".parse().unwrap(),
                ])
            },
        );
    });
    client.write_all(request.as_bytes()).unwrap();
    let mut response = String::new();
    client.read_to_string(&mut response).unwrap();
    gateway.join().unwrap();
    assert!(response.starts_with("HTTP/1.1 403"));
    assert!(restricted.load(Ordering::Acquire));
    assert!(matches!(listener.accept(), Err(error) if error.kind() == io::ErrorKind::WouldBlock));
}

#[test]
fn unrelated_local_requests_cannot_authorize_tunnels_or_poison_the_import() {
    let mut gateway = Gateway::start().unwrap();
    for address in [
        SocketAddr::from((Ipv4Addr::LOCALHOST, gateway.port)),
        SocketAddr::from((Ipv6Addr::LOCALHOST, gateway.port)),
    ] {
        assert!(
            matches!(TcpListener::bind(address), Err(error) if error.kind() == io::ErrorKind::AddrInUse)
        );
        let mut client = TcpStream::connect(address).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        client
            .write_all(b"CONNECT www.youtube.com:443 HTTP/1.1\r\n\r\n")
            .unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 407"));
    }
    assert!(gateway.finish().is_none());
}

#[test]
fn shutdown_drains_bounded_connections_waiting_for_headers() {
    let mut gateway = Gateway::start().unwrap();
    let clients: Vec<_> = (0..MAX_CONNECTIONS + 2)
        .map(|_| TcpStream::connect((Ipv4Addr::LOCALHOST, gateway.port)).unwrap())
        .collect();
    let started = Instant::now();
    assert!(gateway.finish().is_none());
    assert!(started.elapsed() < Duration::from_secs(2));
    drop(clients);
}

#[cfg(target_os = "macos")]
#[test]
fn sandbox_blocks_direct_bypass_and_gateway_rejects_local_http_and_https() {
    use crate::youtube::runner::{HelperCommand, NetworkAccess, Workspace, run_youtube_helper};
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let workspace = Workspace::new().unwrap();
    for (scheme, bypass) in [("http", false), ("https", false), ("http", true)] {
        let mut args: Vec<OsString> = ["--silent", "--show-error", "--max-time", "3"]
            .map(OsString::from)
            .into();
        if bypass {
            args.extend(["--noproxy".into(), "*".into()]);
        }
        args.push(format!("{scheme}://{}/owned", listener.local_addr().unwrap()).into());
        let result = run_youtube_helper(
            HelperCommand {
                executable: Path::new("/usr/bin/curl"),
                arguments: &args,
                private: workspace.path(),
                current_dir: workspace.path(),
                max_stdout: 1024,
                overflow: ImportError::new("DownloaderOutputTooLarge", "test bound"),
                timeout: Duration::from_secs(5),
            },
            NetworkAccess::YouTube,
            &AtomicBool::new(false),
            || Ok(()),
        );
        if bypass {
            assert!(!result.unwrap().status.success());
        } else {
            assert!(
                matches!(result, Err(crate::CliError::Import(error)) if error.code == "YouTubeNetworkRestricted")
            );
        }
        assert!(
            matches!(listener.accept(), Err(error) if error.kind() == io::ErrorKind::WouldBlock)
        );
    }
}
