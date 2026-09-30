use super::*;
use deadpan_core::{NodeId, ProjectDocument, ProjectId, RevisionId};
use deadpan_store::AccessMode;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::thread;

type TestResult = Result<(), Box<dyn std::error::Error>>;
fn store(path: &Path) -> Result<ProjectStore, Box<dyn std::error::Error>> {
    let document = ProjectDocument::new_automatic(
        ProjectId::new("host-project")?,
        RevisionId::new("initial")?,
        NodeId::new("root")?,
    )?;
    Ok(ProjectStore::create(path, &document)?)
}
pub(super) fn discovery(path: &Path) -> namespace::Discovery {
    serde_json::from_slice(
        &deadpan_store::host_owner::read_discovery(path)
            .unwrap()
            .unwrap()
            .bytes,
    )
    .unwrap()
}
pub(super) fn connect(discovery: &namespace::Discovery) -> UnixStream {
    let lease = namespace::Lease::open(discovery).unwrap();
    let fd = wire::socket().unwrap();
    let address = rustix::net::SocketAddrUnix::new(lease.path()).unwrap();
    rustix::net::connect(&fd, &address).unwrap();
    UnixStream::from(fd)
}
pub(super) fn request(discovery: &namespace::Discovery) -> Request {
    Request {
        version: VERSION,
        owner_id: discovery.owner_id,
        secret: discovery.secret.clone(),
        package_identity: discovery.package_identity,
        request_id: Uuid::new_v4(),
        payload: serde_json::json!({"operation":"test"}),
    }
}
pub(super) fn send(stream: &mut UnixStream, value: &impl Serialize) {
    let mut writer = wire::Writer::new(value).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !writer
        .poll(stream, &mut wire::Budget::new(), deadline)
        .unwrap()
    {
        thread::yield_now();
    }
}
fn receive(
    endpoint: &mut Endpoint,
    stream: &mut UnixStream,
    mut dispatch: impl FnMut(Incoming, &mut Endpoint),
) -> Response {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut reader = wire::Reader::default();
    let mut available = MAX_BUFFER_BYTES;
    loop {
        assert!(Instant::now() < deadline, "transport did not complete");
        for incoming in endpoint.poll() {
            dispatch(incoming, endpoint);
        }
        if let Some(bytes) = reader
            .poll(stream, &mut wire::Budget::new(), &mut available, deadline)
            .unwrap()
        {
            return serde_json::from_slice(&bytes).unwrap();
        }
        thread::yield_now();
    }
}

#[test]
fn client_roundtrip_preserves_identity_and_does_not_own_writer() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("project.deadpan");
    let mut writer = store(&path)?;
    let mut endpoint = Endpoint::bind(&mut writer)?;
    let request_id = Uuid::new_v4();
    let mut client = Client::discover(&path)?.unwrap();
    assert_eq!(client.owner_id(), endpoint.owner_id());
    let worker = thread::spawn(move || {
        client.request_until(
            request_id,
            serde_json::json!({"exact":17}),
            Instant::now() + Duration::from_secs(5),
        )
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut count = 0;
    while !worker.is_finished() {
        assert!(Instant::now() < deadline);
        for incoming in endpoint.poll() {
            count += 1;
            assert_eq!(incoming.request_id, request_id);
            assert_eq!(incoming.payload, serde_json::json!({"exact":17}));
            endpoint.respond(incoming.ticket, serde_json::json!({"receipt":"saved"}))?;
        }
        thread::yield_now();
    }
    assert_eq!(
        worker.join().unwrap()?,
        serde_json::json!({"receipt":"saved"})
    );
    assert_eq!(count, 1);
    let idle_client = Client::discover(&path)?.unwrap();
    drop(writer);
    assert!(endpoint.owner_handle().is_closed());
    assert!(endpoint.poll().is_empty());
    assert!(deadpan_store::host_owner::read_discovery(&path)?.is_none());
    let reopened = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    drop(idle_client);
    drop(reopened);
    Ok(())
}

#[test]
fn strict_frames_and_authentication_never_reach_dispatch() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("project.deadpan");
    let mut writer = store(&path)?;
    let mut endpoint = Endpoint::bind(&mut writer)?;
    let discovery = discovery(&path);
    for case in [
        "version",
        "owner",
        "secret",
        "package",
        "request",
        "unknown",
        "malformed",
    ] {
        let mut stream = connect(&discovery);
        let mut value = serde_json::to_value(request(&discovery))?;
        match case {
            "version" => value["version"] = 2.into(),
            "owner" => value["owner_id"] = Uuid::new_v4().to_string().into(),
            "secret" => value["secret"] = "wrong-secret".into(),
            "package" => value["package_identity"]["inode"] = 0.into(),
            "request" => value["request_id"] = Uuid::nil().to_string().into(),
            "unknown" => value["extra"] = true.into(),
            "malformed" => value = Value::Null,
            _ => unreachable!(),
        }
        send(&mut stream, &value);
        let response = receive(&mut endpoint, &mut stream, |_, _| {
            panic!("invalid request dispatched")
        });
        assert!(
            matches!(&response.result, ResponseResult::Error { .. }),
            "{case}"
        );
        let text = serde_json::to_string(&response)?;
        assert!(!text.contains(&discovery.secret));
        assert!(!text.contains("wrong-secret"));
    }
    Ok(())
}

#[test]
fn oversize_header_is_rejected_before_payload_allocation() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("project.deadpan");
    let mut writer = store(&path)?;
    let mut endpoint = Endpoint::bind(&mut writer)?;
    let mut stream = connect(&discovery(&path));
    stream.write_all(&u32::try_from(MAX_FRAME_BYTES + 1)?.to_be_bytes())?;
    let response = receive(&mut endpoint, &mut stream, |_, _| {
        panic!("oversize request dispatched")
    });
    assert!(
        matches!(response.result, ResponseResult::Error { error } if error.code == "HostLimit")
    );
    Ok(())
}

#[test]
fn partial_headers_and_four_connections_leave_dispatch_bounded() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("project.deadpan");
    let mut writer = store(&path)?;
    let mut endpoint = Endpoint::bind(&mut writer)?;
    let discovery = discovery(&path);
    let mut peers: Vec<_> = (0..MAX_CONNECTIONS).map(|_| connect(&discovery)).collect();
    for peer in &mut peers {
        peer.write_all(&[0])?;
    }
    for _ in 0..8 {
        assert!(endpoint.poll().is_empty());
    }
    // A fifth connection waits behind the fixed admitted set. Closing one
    // partial sender releases a slot without waiting for its receive deadline.
    let mut fifth = connect(&discovery);
    let expected = request(&discovery);
    send(&mut fifth, &expected);
    assert!(endpoint.poll().is_empty());
    peers.pop();
    let response = receive(&mut endpoint, &mut fifth, |incoming, endpoint| {
        assert_eq!(incoming.request_id, expected.request_id);
        endpoint.respond(incoming.ticket, true.into()).unwrap();
    });
    assert!(matches!(
        response.result,
        ResponseResult::Ok {
            payload: Value::Bool(true)
        }
    ));
    Ok(())
}

#[test]
fn revoked_owner_and_replaced_namespace_refuse_requests() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("project.deadpan");
    let mut writer = store(&path)?;
    let endpoint = Endpoint::bind(&mut writer)?;
    let mut client = Client::discover(&path)?.unwrap();
    let discovery = discovery(&path);
    let lease = namespace::Lease::open(&discovery)?;
    let socket = lease.path();
    std::fs::set_permissions(
        socket.parent().unwrap(),
        std::fs::Permissions::from_mode(0o755),
    )?;
    assert!(Client::discover(&path).is_err());
    assert_eq!(
        client.request(Value::Null).unwrap_err().code,
        "HostOwnerChanged"
    );
    std::fs::set_permissions(
        socket.parent().unwrap(),
        std::fs::Permissions::from_mode(0o700),
    )?;
    drop(writer);
    assert_eq!(
        client.request(Value::Null).unwrap_err().code,
        "HostOwnerChanged"
    );
    let mut successor = ProjectStore::open(&path, AccessMode::ReadWrite)?;
    let new_endpoint = Endpoint::bind(&mut successor)?;
    drop(endpoint);
    assert_eq!(
        Client::discover(&path)?.unwrap().owner_id(),
        new_endpoint.owner_id()
    );
    Ok(())
}

#[test]
fn lost_reply_after_delivery_is_uncertain_and_not_retried() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("project.deadpan");
    let mut writer = store(&path)?;
    let mut endpoint = Endpoint::bind(&mut writer)?;
    let mut client = Client::discover(&path)?.unwrap();
    let worker = thread::spawn(move || client.request(Value::Null));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(Instant::now() < deadline);
        let admitted = endpoint.poll();
        if !admitted.is_empty() {
            assert_eq!(admitted.len(), 1);
            break;
        }
        thread::yield_now();
    }
    // The owner may have committed. Simulate loss of transport afterward.
    drop(endpoint);
    let result = worker.join().unwrap().unwrap_err();
    assert_eq!(result.code, "HostOutcomeUnknown");
    Ok(())
}

#[test]
fn endpoint_drop_preserves_a_replacement_runtime_directory() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("project.deadpan");
    let mut writer = store(&path)?;
    let endpoint = Endpoint::bind(&mut writer)?;
    let mut client = Client::discover(&path)?.unwrap();
    let lease = namespace::Lease::open(&discovery(&path))?;
    let socket = lease.path();
    let directory = socket.parent().unwrap();
    let moved = directory.with_extension("saved");
    std::fs::rename(directory, &moved)?;
    std::fs::create_dir(directory)?;
    std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o700))?;
    assert_eq!(
        client.request(Value::Null).unwrap_err().code,
        "HostOwnerChanged"
    );
    drop(endpoint);
    assert!(
        directory.is_dir(),
        "old endpoint removed a successor namespace"
    );
    std::fs::remove_dir(directory)?;
    std::fs::remove_file(moved.join("control.sock"))?;
    std::fs::remove_dir(moved)?;
    Ok(())
}

#[test]
fn descriptor_setup_and_deadline_checks_are_explicit() -> TestResult {
    let fd = wire::socket()?;
    assert!(rustix::io::fcntl_getfd(&fd)?.contains(rustix::io::FdFlags::CLOEXEC));
    assert!(rustix::fs::fcntl_getfl(&fd)?.contains(rustix::fs::OFlags::NONBLOCK));
    let (mut left, mut right) = UnixStream::pair()?;
    left.set_nonblocking(true)?;
    right.set_nonblocking(true)?;
    right.write_all(&[0, 0])?;
    let mut reader = wire::Reader::default();
    let mut available = MAX_BUFFER_BYTES;
    assert!(
        reader
            .poll(
                &mut left,
                &mut wire::Budget::new(),
                &mut available,
                Instant::now() + RECEIVE_TIMEOUT
            )?
            .is_none()
    );
    assert!(reader.length.is_none());
    assert_eq!(available, MAX_BUFFER_BYTES);
    assert_eq!(
        reader
            .poll(
                &mut left,
                &mut wire::Budget::new(),
                &mut available,
                Instant::now()
            )
            .unwrap_err()
            .code,
        "HostTimeout"
    );
    let mut outgoing = wire::Writer::new(&true)?;
    assert_eq!(
        outgoing
            .poll(&mut left, &mut wire::Budget::new(), Instant::now())
            .unwrap_err()
            .code,
        "HostTimeout"
    );
    assert!(!outgoing.started());
    assert!(wire::Writer::limited(&serde_json::json!({"large":"payload"}), 8).is_err());
    let scalar = br#"{"value":"[,,,,,,,,,,,,]"}"#;
    let expanded = br#"{"value":[0,0,0,0,0,0,0]}"#;
    assert!(wire::parsed_reservation(expanded)? > wire::parsed_reservation(scalar)?);
    let mut witness = std::process::Command::new("sh");
    witness
        .args(["-c", "test ! -e \"$1\"", "host-fd-witness"])
        .arg(format!("/dev/fd/{}", fd.as_raw_fd()))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    assert!(
        deadpan_native_process::spawn(&mut witness)?
            .wait()?
            .success(),
        "host socket inherited across participating spawn"
    );
    Ok(())
}

#[test]
fn dense_json_is_refused_locally_before_any_request_bytes_are_sent() -> TestResult {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("project.deadpan");
    let mut writer = store(&path)?;
    let mut endpoint = Endpoint::bind(&mut writer)?;
    let mut client = Client::discover(&path)?.unwrap();
    // Text is only about 5 MiB, but owned Value allocation accounting exceeds
    // 256 MiB. This must fail before transmission, not after a possible commit.
    let dense = Value::Array(vec![Value::Null; MAX_BUFFER_BYTES / 256]);
    assert_eq!(client.request(dense).unwrap_err().code, "HostLimit");
    for _ in 0..4 {
        assert!(endpoint.poll().is_empty());
    }
    Ok(())
}

#[test]
fn symlink_discovery_pins_the_resolved_package_across_alias_retargeting() -> TestResult {
    let temp = tempfile::tempdir()?;
    let first_path = temp.path().join("first.deadpan");
    let second_path = temp.path().join("second.deadpan");
    let alias = temp.path().join("alias.deadpan");
    let mut first_store = store(&first_path)?;
    let mut second_store = store(&second_path)?;
    let mut first = Endpoint::bind(&mut first_store)?;
    let mut second = Endpoint::bind(&mut second_store)?;
    std::os::unix::fs::symlink(&first_path, &alias)?;
    let mut pinned = Client::discover(&alias)?.expect("alias resolves to owner");
    assert_eq!(pinned.owner_id(), first.owner_id());
    assert_eq!(pinned.package_identity(), first.package_identity());

    std::fs::remove_file(&alias)?;
    std::os::unix::fs::symlink(&second_path, &alias)?;
    let later = Client::discover(&alias)?.expect("new discovery follows new alias");
    assert_eq!(later.owner_id(), second.owner_id());
    assert_ne!(later.package_identity(), pinned.package_identity());
    let worker =
        thread::spawn(move || pinned.request(serde_json::json!({"operation":"original-owner"})));
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut delivered = 0;
    while !worker.is_finished() {
        assert!(Instant::now() < deadline);
        for incoming in first.poll() {
            delivered += 1;
            assert_eq!(incoming.payload["operation"], "original-owner");
            first.respond(incoming.ticket, serde_json::json!({"owner":"first"}))?;
        }
        assert!(
            second.poll().is_empty(),
            "existing client followed the retargeted alias"
        );
        thread::yield_now();
    }
    assert_eq!(
        worker.join().unwrap()?,
        serde_json::json!({"owner":"first"})
    );
    assert_eq!(delivered, 1);
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn accept_returns_would_block_while_a_spawn_owns_descriptor_creation() -> TestResult {
    let socket = wire::socket()?;
    deadpan_native_process::with_descriptor_creation_guard(|| {
        // This socket is deliberately not listening. Contention must return
        // WouldBlock before attempting accept or creating another descriptor.
        let error = wire::accept(&socket).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::WouldBlock);
        Ok(())
    })?;
    Ok(())
}
