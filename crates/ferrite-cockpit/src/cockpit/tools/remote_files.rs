//! How Files reaches a checkout on another machine
//! (`ferrite_tools::remote_files`): the window's `Submit`, each request
//! sent over its connection to the machine, answered off the UI thread.

use ferrite_client::cockpit::Cockpit;
use ferrite_core::file_operations::Request;
use ferrite_tools::remote_files::{Answer, Submit};
use futures::channel::oneshot;
use gpui::{App, WeakEntity};
use std::io;
use std::rc::Rc;
use std::sync::{mpsc, Arc};
use std::time::Duration;

/// Each request to `machine` through `cockpit`, while its connection is
/// still `bridge`.
pub(super) fn submit(
    cockpit: WeakEntity<Cockpit>,
    machine: ferrite_core::MachineId,
    bridge: Arc<ferrite_core::machine::Bridge>,
) -> Rc<Submit> {
    Rc::new(move |request: Request, cx: &mut App| -> Answer {
        let (send, answer) = oneshot::channel();
        let cockpit = cockpit.clone();
        let bridge = bridge.clone();
        cx.defer(move |cx| {
            let requested = cockpit
                .update(cx, |cockpit, _| {
                    let current = cockpit.bridge(machine);
                    if !current.is_some_and(|current| Arc::ptr_eq(current, &bridge)) {
                        return Err(io::Error::other(
                            "This connection changed. Reopen the file on its machine.",
                        ));
                    }
                    cockpit.file_operation(machine, request)
                })
                .map_err(|error| io::Error::other(error.to_string()))
                .and_then(|result| result);
            match requested {
                Ok(receiver) => {
                    let executor = cx.background_executor().clone();
                    let timer = executor.clone();
                    executor.spawn(async move {
                        let started = timer.now();
                        let result = loop {
                            match receiver.try_recv() {
                                Ok(reply) => break reply,
                                Err(mpsc::TryRecvError::Disconnected) => break Err(io::Error::other("The remote connection closed before answering.")),
                                Err(mpsc::TryRecvError::Empty) => {}
                            }
                            if timer.now().duration_since(started) >= Duration::from_secs(30) {
                                break Err(io::Error::new(io::ErrorKind::TimedOut, "The remote file operation did not finish. Reconnect and retry."));
                            }
                            timer.timer(Duration::from_millis(20)).await;
                        };
                        let _ = send.send(result);
                    }).detach();
                }
                Err(error) => {
                    let _ = send.send(Err(error));
                }
            }
        });
        answer
    })
}
