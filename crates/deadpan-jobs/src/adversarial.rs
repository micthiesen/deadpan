//! Gate G adversarial regression shared by the framed worker protocols. Each
//! protocol supplies valid host requests and worker responses; inputs are
//! mutated frame streams read by the worker-side host reader (selector 0) or
//! the host-side response reader plus identity classification (selector 1).
//! Every stream must end cleanly or with a typed error, never a panic, within
//! the per-case bounds. See docs/ADVERSARIAL.md.

use crate::process::WorkerProtocol;
use deadpan_chaos::{Target, Verdict, fuzz, read_stream, select};
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
    let classifier = P::from_request(&requests[0]).expect("seed request is valid");
    let mut seeds = Vec::new();
    for request in &requests {
        seeds.push(frames(0, std::slice::from_ref(request)));
    }
    seeds.push(frames(0, &requests));
    for response in &responses {
        seeds.push(frames(1, std::slice::from_ref(response)));
    }
    seeds.push(frames(1, &responses));
    let report = fuzz(Target::frames(name), seeds, |input| {
        let (selector, stream) = select(input);
        if selector % 2 == 0 {
            read_stream(stream, 64, |reader| {
                let request = read_host(reader)?;
                Ok(request.map(|_| ()))
            })
        } else {
            let mut kinds = Vec::new();
            let verdict = read_stream(stream, 64, |reader: &mut &[u8]| {
                let response = P::read_response(reader)?;
                match response {
                    Some(response) => {
                        kinds.push(classifier.classify(&response).is_ok());
                        Ok(Some(()))
                    }
                    None => Ok(None),
                }
            })?;
            Ok(match verdict {
                Verdict::Accepted if kinds.iter().any(|ok| !ok) => {
                    Verdict::Rejected("valid frame for another attempt".into())
                }
                other => other,
            })
        }
    });
    report.assert_clean();
}
