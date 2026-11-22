use crate::cockpit::CockpitView;
use ferrite_core::file_operations::{Fingerprint, Reply, Request};
use futures::channel::oneshot;
use gpui::{App, WeakEntity};
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{mpsc, Arc};
use std::time::Duration;

type Answer = oneshot::Receiver<io::Result<Reply>>;
type Submit = dyn Fn(Request, &mut App) -> Answer;

#[derive(Clone)]
pub(in crate::cockpit::tools) struct RemoteFiles {
    root: PathBuf,
    submit: Rc<Submit>,
}

impl RemoteFiles {
    pub(in crate::cockpit::tools) fn new(
        root: PathBuf,
        view: WeakEntity<CockpitView>,
        machine: ferrite_core::MachineId,
        bridge: Arc<ferrite_core::machine::Bridge>,
    ) -> Self {
        Self {
            root,
            submit: Rc::new(move |request, cx| {
                let (send, answer) = oneshot::channel();
                let view = view.clone();
                let bridge = bridge.clone();
                cx.defer(move |cx| {
                    let requested = view
                        .update(cx, |view, cx| {
                            let current = view.cockpit.read(cx).bridge(machine);
                            if !current.is_some_and(|current| Arc::ptr_eq(current, &bridge)) {
                                return Err(io::Error::other(
                                    "This connection changed. Reopen the file on its machine.",
                                ));
                            }
                            view.cockpit
                                .update(cx, |cockpit, _| cockpit.file_operation(machine, request))
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
            }),
        }
    }

    pub(in crate::cockpit::tools) fn read(&self, path: &Path, cx: &mut App) -> Answer {
        (self.submit)(
            Request::Read {
                root: self.root.clone(),
                path: path.to_path_buf(),
            },
            cx,
        )
    }

    pub(in crate::cockpit::tools) fn write(
        &self,
        path: &Path,
        text: String,
        expected: Option<Fingerprint>,
        cx: &mut App,
    ) -> Answer {
        (self.submit)(
            Request::Write {
                root: self.root.clone(),
                path: path.to_path_buf(),
                text,
                expected,
            },
            cx,
        )
    }

    pub(in crate::cockpit::tools) fn snapshot(
        &self,
        directories: Vec<PathBuf>,
        search: bool,
        cx: &mut App,
    ) -> Answer {
        (self.submit)(
            Request::Snapshot {
                root: self.root.clone(),
                directories,
                search,
            },
            cx,
        )
    }

    pub(in crate::cockpit::tools) fn same_workspace(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.submit, &other.submit)
    }

    #[cfg(test)]
    pub(in crate::cockpit::tools) fn fake(
        root: PathBuf,
        reply: impl Fn(Request) -> io::Result<Reply> + 'static,
    ) -> Self {
        Self {
            root,
            submit: Rc::new(move |request, _| {
                let (send, answer) = oneshot::channel();
                let _ = send.send(reply(request));
                answer
            }),
        }
    }
}

pub(in crate::cockpit::tools) async fn receive(answer: Answer) -> io::Result<Reply> {
    answer
        .await
        .map_err(|_| io::Error::other("The remote file operation was cancelled."))?
}
