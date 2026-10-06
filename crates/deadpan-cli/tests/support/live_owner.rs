//! A stand-in for the native app as project owner: it holds the writer and
//! serves the authenticated live endpoint with the shared executor, so a CLI
//! subprocess takes exactly the open-project route the app serves.

use std::thread;
use std::time::{Duration, Instant};

use deadpan_cli::host::Endpoint;
use deadpan_cli::live_project::{
    LiveError, Operation, Reply, Request, ShortOperation, execute_short,
};
use deadpan_cli::render::RenderContext;
use deadpan_store::ProjectStore;

/// Run `client` (typically a CLI subprocess) while `store` serves its short
/// operations. Returns the client's result and how many operations ran.
pub fn serve<T: Send>(
    store: &mut ProjectStore,
    endpoint: &mut Endpoint,
    client: impl FnOnce() -> T + Send,
) -> (T, usize) {
    thread::scope(|scope| {
        let client = scope.spawn(client);
        let deadline = Instant::now() + Duration::from_secs(120);
        let mut executed = 0;
        while !client.is_finished() {
            assert!(Instant::now() < deadline, "live owner fixture timed out");
            for incoming in endpoint.poll() {
                let reply = match Request::from_value(incoming.payload) {
                    Ok(Request {
                        operation: Operation::Inspect,
                        ..
                    }) => Reply::Context {
                        context: RenderContext::from_document(&store.snapshot().unwrap()),
                        preview_active: false,
                    },
                    Ok(Request {
                        operation:
                            Operation::Execute {
                                project_id,
                                command,
                            },
                        ..
                    }) => {
                        executed += 1;
                        let restore = matches!(*command, ShortOperation::RestoreBackup { .. });
                        let result = execute_short(store, &project_id, &command);
                        if restore && result.is_ok() {
                            // As the app does: the restore revoked this owner;
                            // its endpoint only writes the admitted reply.
                            endpoint.retire();
                        }
                        match result {
                            Ok(execution) => Reply::Completed {
                                output: execution.output,
                                committed_revision: execution.committed_revision,
                                committed_registers: execution.committed_registers,
                                refresh_error: None,
                            },
                            Err(error) => Reply::Failed { error },
                        }
                    }
                    Ok(_) => Reply::Failed {
                        error: LiveError::new("UnexpectedTestOperation", "Short commands only"),
                    },
                    Err(error) => Reply::Failed { error },
                };
                endpoint
                    .respond(incoming.ticket, serde_json::to_value(reply).unwrap())
                    .unwrap();
            }
            thread::park_timeout(Duration::from_millis(1));
        }
        (client.join().expect("client panicked"), executed)
    })
}
