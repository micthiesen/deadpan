use super::*;
use deadpan_core::{NodeId, ProjectDocument, ProjectId, RevisionId};
use std::thread;

type TestResult = Result<(), Box<dyn std::error::Error>>;
fn fixture() -> Result<(tempfile::TempDir, ProjectStore, Endpoint), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("project.deadpan");
    let document = ProjectDocument::new_automatic(
        ProjectId::new("transport")?,
        RevisionId::new("initial")?,
        NodeId::new("root")?,
    )?;
    let mut store = ProjectStore::create(&path, &document)?;
    let endpoint = Endpoint::bind(&mut store)?;
    Ok((temp, store, endpoint))
}

#[test]
fn wrong_response_owner_or_request_is_an_unknown_outcome() -> TestResult {
    for wrong_owner in [false, true] {
        let (temp, _store, mut endpoint) = fixture()?;
        let mut client = Client::discover(&temp.path().join("project.deadpan"))?.unwrap();
        let worker = thread::spawn(move || client.request(Value::Null));
        let deadline = Instant::now() + Duration::from_secs(5);
        let incoming = loop {
            assert!(Instant::now() < deadline);
            if let Some(incoming) = endpoint.poll().pop() {
                break incoming;
            }
            thread::yield_now();
        };
        let response = Response {
            version: VERSION,
            owner_id: if wrong_owner {
                Uuid::new_v4()
            } else {
                endpoint.owner_id()
            },
            package_identity: endpoint.discovery.package_identity,
            request_id: if wrong_owner {
                incoming.request_id
            } else {
                Uuid::new_v4()
            },
            result: ResponseResult::Ok {
                payload: true.into(),
            },
        };
        let connection = &mut endpoint.connections[0];
        assert!(
            rustix::io::fcntl_getfd(&connection.stream)?.contains(rustix::io::FdFlags::CLOEXEC)
        );
        assert!(
            rustix::fs::fcntl_getfl(&connection.stream)?.contains(rustix::fs::OFlags::NONBLOCK)
        );
        connection.state = State::Writing(Writer::new(&response)?);
        while !worker.is_finished() {
            assert!(Instant::now() < deadline);
            assert!(endpoint.poll().is_empty());
            thread::yield_now();
        }
        assert_eq!(
            worker.join().unwrap().unwrap_err().code,
            "HostOutcomeUnknown"
        );
    }
    Ok(())
}

#[test]
fn expired_tickets_and_tickets_from_another_endpoint_cannot_reply() -> TestResult {
    let (temp, _store, mut endpoint) = fixture()?;
    let mut client = Client::discover(&temp.path().join("project.deadpan"))?.unwrap();
    let worker = thread::spawn(move || client.request(Value::Null));
    let deadline = Instant::now() + Duration::from_secs(5);
    let incoming = loop {
        assert!(Instant::now() < deadline);
        if let Some(incoming) = endpoint.poll().pop() {
            break incoming;
        }
        thread::yield_now();
    };
    let foreign = ConnectionTicket {
        endpoint: Uuid::new_v4(),
        serial: incoming.ticket.serial,
    };
    assert!(endpoint.respond(foreign, Value::Null).is_err());
    endpoint.connections[0].deadline = Instant::now();
    assert!(endpoint.respond(incoming.ticket, Value::Null).is_err());
    assert!(endpoint.poll().is_empty());
    assert!(endpoint.connections.is_empty());
    assert_eq!(
        worker.join().unwrap().unwrap_err().code,
        "HostOutcomeUnknown"
    );
    Ok(())
}

#[test]
fn unread_large_reply_does_not_prevent_another_connection_completing() -> TestResult {
    let (temp, _store, mut endpoint) = fixture()?;
    let path = temp.path().join("project.deadpan");
    let discovery = crate::host::tests::discovery(&path);
    let mut slow = crate::host::tests::connect(&discovery);
    crate::host::tests::send(&mut slow, &crate::host::tests::request(&discovery));
    let deadline = Instant::now() + Duration::from_secs(5);
    let slow_ticket = loop {
        assert!(Instant::now() < deadline);
        if let Some(incoming) = endpoint.poll().pop() {
            break incoming.ticket;
        }
        thread::yield_now();
    };
    rustix::net::sockopt::set_socket_send_buffer_size(
        &endpoint
            .connections
            .iter()
            .find(|c| c.ticket == slow_ticket)
            .unwrap()
            .stream,
        4096,
    )?;
    endpoint.respond(slow_ticket, Value::String("x".repeat(2 * 1024 * 1024)))?;
    for _ in 0..8 {
        assert!(endpoint.poll().is_empty());
    }
    assert!(
        endpoint
            .connections
            .iter()
            .any(|c| c.ticket == slow_ticket && matches!(&c.state, State::Writing(_)))
    );
    let mut client = Client::discover(&path)?.unwrap();
    let worker = thread::spawn(move || client.request(17.into()));
    while !worker.is_finished() {
        assert!(Instant::now() < deadline);
        for incoming in endpoint.poll() {
            endpoint.respond(incoming.ticket, incoming.payload)?;
        }
        thread::yield_now();
    }
    assert_eq!(worker.join().unwrap()?, Value::from(17));
    // Expiration frees the stalled reply without a blocking flush.
    endpoint
        .connections
        .iter_mut()
        .find(|c| c.ticket == slow_ticket)
        .unwrap()
        .deadline = Instant::now();
    endpoint.poll();
    assert!(endpoint.connections.iter().all(|c| c.ticket != slow_ticket));
    Ok(())
}

#[test]
fn failed_large_reply_keeps_ticket_and_compact_receipt_capacity() -> TestResult {
    let (temp, _store, mut endpoint) = fixture()?;
    let mut client = Client::discover(&temp.path().join("project.deadpan"))?.unwrap();
    let worker = thread::spawn(move || client.request(Value::Null));
    let deadline = Instant::now() + Duration::from_secs(5);
    let incoming = loop {
        assert!(Instant::now() < deadline);
        if let Some(incoming) = endpoint.poll().pop() {
            break incoming;
        }
        thread::yield_now();
    };
    // Exercise actual aggregate accounting with three controlled reservations
    // for already-admitted parsed requests. Do not allocate hundreds of MiB
    // just to occupy the protocol's accounting boundary.
    let remaining = DATA_BUFFER_BYTES - endpoint.connections[0].reserved;
    let mut peers = Vec::new();
    for index in 0..3 {
        let (stream, peer) = UnixStream::pair()?;
        stream.set_nonblocking(true)?;
        peers.push(peer);
        let reserved = remaining / 3 + usize::from(index == 2) * (remaining % 3);
        endpoint.connections.push(Connection {
            ticket: ConnectionTicket {
                endpoint: endpoint.owner_id(),
                serial: 100 + index,
            },
            stream,
            state: State::Awaiting,
            deadline,
            request_id: Uuid::new_v4(),
            reserved,
        });
    }
    assert_eq!(
        endpoint
            .connections
            .iter()
            .map(|c| c.reserved)
            .sum::<usize>(),
        DATA_BUFFER_BYTES
    );
    let error = endpoint
        .respond(
            incoming.ticket,
            Value::String("x".repeat(FALLBACK_REPLY_BYTES * 2)),
        )
        .unwrap_err();
    assert_eq!(error.code, "HostLimit");
    assert!(matches!(endpoint.connections[0].state, State::Awaiting));
    let receipt = serde_json::json!({"committed_revision":"already-saved","code":"HostReplyLimit"});
    endpoint.respond(incoming.ticket, receipt.clone())?;
    assert!(
        endpoint
            .connections
            .iter()
            .map(|c| c.reserved)
            .sum::<usize>()
            <= MAX_BUFFER_BYTES
    );
    while !worker.is_finished() {
        assert!(Instant::now() < deadline);
        assert!(endpoint.poll().is_empty());
        thread::yield_now();
    }
    assert_eq!(worker.join().unwrap()?, receipt);
    drop(peers);
    Ok(())
}

#[test]
fn a_retired_endpoint_answers_only_its_admitted_request_after_the_owner_closes() -> TestResult {
    for retire in [false, true] {
        let (temp, store, mut endpoint) = fixture()?;
        let mut client = Client::discover(&temp.path().join("project.deadpan"))?.unwrap();
        let worker = thread::spawn(move || client.request(Value::Null));
        let deadline = Instant::now() + Duration::from_secs(5);
        let incoming = loop {
            assert!(Instant::now() < deadline);
            if let Some(incoming) = endpoint.poll().pop() {
                break incoming;
            }
            thread::yield_now();
        };
        // The operation replaced the owner, as a restore does.
        drop(store);
        if retire {
            endpoint.retire();
            endpoint.respond(incoming.ticket, true.into())?;
            assert!(endpoint.draining());
            // The listener is closed: a new client fails at once.
            let started = Instant::now();
            let late = Client::discover(&temp.path().join("project.deadpan"))
                .ok()
                .flatten()
                .map(|mut late| late.request(Value::Null));
            assert!(late.is_none_or(|reply| reply.is_err()));
            assert!(started.elapsed() < Duration::from_secs(2));
        } else {
            assert!(endpoint.respond(incoming.ticket, true.into()).is_err());
        }
        while !worker.is_finished() {
            assert!(Instant::now() < deadline);
            assert!(endpoint.poll().is_empty(), "nothing new is admitted");
            thread::yield_now();
        }
        let reply = worker.join().unwrap();
        if retire {
            assert_eq!(reply?, Value::Bool(true));
            assert!(!endpoint.draining());
        } else {
            assert!(reply.is_err());
        }
    }
    Ok(())
}
