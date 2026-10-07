//! Gate G adversarial regression shared by the CLI's framed worker protocols
//! (render, encoded render, verification, encoder admission probe). Each
//! protocol supplies valid host requests and worker responses; inputs are
//! mutated streams read as requests (0), responses (1), or an initial request
//! followed by responses classified against that request (2). The stable and
//! nightly runners share the same exercise and input interpretation.
//! Every stream must end cleanly or with a typed error, never a panic, within
//! the per-case bounds. See docs/ADVERSARIAL.md.

use deadpan_chaos::{Target, fuzz, protocol_stream};
use deadpan_jobs::process::WorkerProtocol;
use serde::Serialize;

#[global_allocator]
static ALLOCATOR: deadpan_chaos::CountingAllocator = deadpan_chaos::CountingAllocator;

fn frames<T: Serialize>(selector: u8, messages: &[T]) -> Vec<u8> {
    let mut bytes = vec![selector];
    for message in messages {
        bytes.extend(deadpan_chaos::frame(
            &serde_json::to_vec(message).expect("seed serializes"),
        ));
    }
    bytes
}

/// A worker-side reader of host requests from a byte stream.
type HostReader<P> = fn(&mut &[u8]) -> Result<Option<<P as WorkerProtocol>::Request>, String>;

/// Fuzzes one framed protocol. `requests[0]` must be a valid initial request.
pub(crate) fn protocol<P>(
    name: &'static str,
    requests: Vec<P::Request>,
    responses: Vec<P::Response>,
    read_host: HostReader<P>,
) where
    P: WorkerProtocol,
    P::Request: Serialize,
    P::Response: Serialize,
{
    P::from_request(&requests[0]).expect("seed request is valid");
    let mut seeds = Vec::new();
    for request in &requests {
        seeds.push(frames(0, std::slice::from_ref(request)));
    }
    seeds.push(frames(0, &requests));
    for response in &responses {
        seeds.push(frames(1, std::slice::from_ref(response)));
    }
    seeds.push(frames(1, &responses));
    let mut classified = frames(2, std::slice::from_ref(&requests[0]));
    classified.extend_from_slice(&frames(1, &responses)[1..]);
    seeds.push(classified.clone());
    let exercise = |input: &[u8], calls: &mut usize| {
        protocol_stream(
            input,
            read_host,
            |writer, request| P::write_request(writer, request),
            |reader| P::read_response(reader),
            |request| P::from_request(request).map_err(|error| error.to_string()),
            |protocol, response| {
                *calls += 1;
                protocol.classify(response).map(|_| ())
            },
        )
    };
    let mut calls = 0;
    exercise(&classified, &mut calls).expect("classified seed does not violate invariants");
    assert!(
        calls > 0,
        "valid request/response seed never reached classification"
    );
    let report = fuzz(Target::frames(name), seeds, |input| exercise(input, &mut 0));
    report.assert_clean();
}
