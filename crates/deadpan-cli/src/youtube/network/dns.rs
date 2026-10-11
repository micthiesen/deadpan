//! One bounded DNS worker shared by YouTube imports and thumbnails. A timed-out OS
//! lookup keeps its slot until it actually returns, so repeated cancellation
//! cannot leave an unbounded collection of resolver threads behind.

use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, SyncSender};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

static LOOKUP: LazyLock<Option<Lookup>> = LazyLock::new(|| {
    Lookup::start(|host| {
        (host, 443)
            .to_socket_addrs()
            .map(|addresses| addresses.take(16).collect())
            .unwrap_or_default()
    })
});

pub(in crate::youtube) fn resolve(
    host: &str,
    timeout: Duration,
) -> Result<Vec<SocketAddr>, LookupError> {
    LOOKUP
        .as_ref()
        .ok_or(LookupError::Unavailable)?
        .query(host, timeout)
}

struct Request {
    host: String,
    reply: SyncSender<Vec<SocketAddr>>,
}

struct Lookup {
    sender: SyncSender<Request>,
    busy: Arc<AtomicBool>,
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::youtube) enum LookupError {
    Timeout,
    Unavailable,
}

impl Lookup {
    fn start(mut lookup: impl FnMut(&str) -> Vec<SocketAddr> + Send + 'static) -> Option<Self> {
        let (sender, receiver) = mpsc::sync_channel::<Request>(1);
        let busy = Arc::new(AtomicBool::new(false));
        let active = busy.clone();
        std::thread::Builder::new()
            .name("deadpan-youtube-dns".into())
            .spawn(move || {
                while let Ok(request) = receiver.recv() {
                    let addresses = lookup(&request.host);
                    active.store(false, Ordering::Release);
                    let _ = request.reply.send(addresses);
                }
            })
            .ok()?;
        Some(Self { sender, busy })
    }

    fn query(&self, host: &str, timeout: Duration) -> Result<Vec<SocketAddr>, LookupError> {
        if self
            .busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(LookupError::Unavailable);
        }
        let (reply, receiver) = mpsc::sync_channel(1);
        if self
            .sender
            .try_send(Request {
                host: host.into(),
                reply,
            })
            .is_err()
        {
            self.busy.store(false, Ordering::Release);
            return Err(LookupError::Unavailable);
        }
        receiver.recv_timeout(timeout).map_err(|error| match error {
            RecvTimeoutError::Timeout => LookupError::Timeout,
            RecvTimeoutError::Disconnected => LookupError::Unavailable,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::Instant;

    #[test]
    fn a_timed_out_lookup_retains_the_only_worker_slot_until_it_finishes() {
        let (release, wait) = mpsc::sync_channel(1);
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let lookup = Lookup::start(move |_| {
            count.fetch_add(1, Ordering::Relaxed);
            wait.recv().unwrap();
            vec!["142.250.1.1:443".parse().unwrap()]
        })
        .unwrap();
        assert_eq!(
            lookup.query("i.ytimg.com", Duration::from_millis(20)),
            Err(LookupError::Timeout)
        );
        assert_eq!(
            lookup.query("i.ytimg.com", Duration::from_millis(20)),
            Err(LookupError::Unavailable)
        );
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while lookup.busy.load(Ordering::Acquire) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(!lookup.busy.load(Ordering::Acquire));
        release.send(()).unwrap();
        assert_eq!(
            lookup.query("i.ytimg.com", Duration::from_secs(2)).unwrap(),
            vec!["142.250.1.1:443".parse::<SocketAddr>().unwrap()]
        );
        assert_eq!(calls.load(Ordering::Relaxed), 2);
    }
}
