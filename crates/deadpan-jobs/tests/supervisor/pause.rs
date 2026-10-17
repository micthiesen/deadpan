use super::*;

fn paused_process(request: HostMessage) -> (tempfile::TempDir, WorkerProcess) {
    paused_process_mode(request, "pause-cancel")
}

fn paused_process_mode(request: HostMessage, mode: &str) -> (tempfile::TempDir, WorkerProcess) {
    let protocol = request.protocol();
    let workspace = tempfile::tempdir().unwrap();
    let ack = WorkerMessage::PauseState {
        protocol,
        identity: identity(),
        sequence: 1,
        paused: true,
    };
    let mut bytes = Vec::new();
    write_worker_message(&mut bytes, &ack).unwrap();
    fs::write(workspace.path().join("pause.bin"), bytes).unwrap();
    responses(
        workspace.path(),
        &[WorkerMessage::Cancelled {
            protocol,
            identity: identity(),
        }],
    );
    let mut specification = spec(workspace.path(), mode);
    specification.limits.cancellation_grace = Duration::from_secs(2);
    let mut process = WorkerProcess::spawn(specification, request).unwrap();
    assert!(process.request_pause(1, true).unwrap());
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(Instant::now() < deadline, "pause acknowledgement missing");
        let events = process.poll(Instant::now()).unwrap();
        assert!(faults(&events).is_empty(), "{:?}", faults(&events));
        if events
            .iter()
            .any(|event| matches!(event, ProcessEvent::Message(message) if **message == ack))
        {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    (workspace, process)
}

#[test]
fn cooperative_pause_discounts_compute_deadline_and_cancellation_still_reaches_worker() {
    for request in [bridge_request(), extension::extension_request()] {
        let protocol = request.protocol();
        let (workspace, mut process) = paused_process(request);
        // An acknowledged hold can exceed the ten-second compute budget.
        let events = process
            .poll(Instant::now() + Duration::from_secs(20))
            .unwrap();
        assert!(faults(&events).is_empty());
        assert!(!process.is_finished());
        assert!(process.request_cancel(Instant::now()).unwrap());
        assert!(!process.request_pause(2, false).unwrap());
        let events = finish(&mut process);
        assert!(faults(&events).is_empty(), "{:?}", faults(&events));
        let received: HostMessage =
            serde_json::from_slice(&fs::read(workspace.path().join("cancellation.json")).unwrap())
                .unwrap();
        assert_eq!(
            received,
            HostMessage::Cancel {
                protocol,
                identity: identity(),
                cancellation_token: CancellationToken::new("cancel").unwrap()
            }
        );
        let received: HostMessage =
            serde_json::from_slice(&fs::read(workspace.path().join("pause-request.json")).unwrap())
                .unwrap();
        assert_eq!(
            received,
            HostMessage::SetPaused {
                protocol,
                identity: identity(),
                cancellation_token: CancellationToken::new("cancel").unwrap(),
                sequence: 1,
                paused: true
            }
        );
    }
}

#[test]
fn cooperative_pause_has_a_real_bounded_lifetime() {
    let (_workspace, mut process) = paused_process(bridge_request());
    let events = process
        .poll(Instant::now() + Duration::from_secs(2 * 60 * 60 + 1))
        .unwrap();
    assert!(
        faults(&events)
            .iter()
            .any(|reason| reason.contains("maximum acknowledged pause"))
    );
    finish(&mut process);
}

#[test]
fn unsolicited_pause_acknowledgement_is_a_protocol_fault() {
    let workspace = tempfile::tempdir().unwrap();
    responses(
        workspace.path(),
        &[WorkerMessage::PauseState {
            protocol: ProtocolVersion::V2,
            identity: identity(),
            sequence: 1,
            paused: true,
        }],
    );
    let mut process =
        WorkerProcess::spawn(spec(workspace.path(), "messages"), bridge_request()).unwrap();
    let events = finish(&mut process);
    assert!(
        faults(&events)
            .iter()
            .any(|reason| reason.contains("sent request"))
    );
}

#[test]
fn rapid_pause_resume_pause_delivers_superseded_physical_resume_before_next_pause() {
    let (workspace, mut process) = paused_process_mode(bridge_request(), "pause-toggle-cancel");
    for (sequence, paused) in [(2, false), (3, true)] {
        let mut bytes = Vec::new();
        write_worker_message(
            &mut bytes,
            &WorkerMessage::PauseState {
                protocol: ProtocolVersion::V2,
                identity: identity(),
                sequence,
                paused,
            },
        )
        .unwrap();
        fs::write(
            workspace.path().join(format!("pause-{sequence}.bin")),
            bytes,
        )
        .unwrap();
    }
    assert!(process.request_pause(2, false).unwrap());
    // No polling between requests: resume is in flight when pause supersedes it.
    assert!(process.request_pause(3, true).unwrap());
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut acknowledgements = Vec::new();
    while acknowledgements.len() < 2 {
        assert!(
            Instant::now() < deadline,
            "physical scheduling state was suppressed"
        );
        let events = process.poll(Instant::now()).unwrap();
        assert!(faults(&events).is_empty(), "{:?}", faults(&events));
        for event in events {
            if let ProcessEvent::Message(message) = event
                && let WorkerMessage::PauseState {
                    sequence, paused, ..
                } = *message
            {
                acknowledgements.push((sequence, paused));
            }
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(acknowledgements, [(2, false), (3, true)]);
    process.request_cancel(Instant::now()).unwrap();
    assert!(faults(&finish(&mut process)).is_empty());
}
