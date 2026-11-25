//! A checkout on another machine, as Files reads and writes it: each
//! operation goes out through the `Submit` the window makes over its
//! connection to that machine, and comes back as an `Answer`.

use ferrite_core::file_operations::{Fingerprint, Reply, Request};
use futures::channel::oneshot;
use gpui::App;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub type Answer = oneshot::Receiver<io::Result<Reply>>;
/// How a request reaches the machine: the window's, over its connection.
pub type Submit = dyn Fn(Request, &mut App) -> Answer;

#[derive(Clone)]
pub struct RemoteFiles {
    root: PathBuf,
    submit: Rc<Submit>,
}

impl RemoteFiles {
    /// The checkout at `root` on the machine `submit` reaches.
    pub fn new(root: PathBuf, submit: Rc<Submit>) -> Self {
        Self { root, submit }
    }

    pub fn read(&self, path: &Path, cx: &mut App) -> Answer {
        (self.submit)(
            Request::Read {
                root: self.root.clone(),
                path: path.to_path_buf(),
            },
            cx,
        )
    }

    pub fn write(&self, path: &Path, text: String, expected: Option<Fingerprint>, cx: &mut App) -> Answer {
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

    pub fn snapshot(&self, directories: Vec<PathBuf>, search: bool, cx: &mut App) -> Answer {
        (self.submit)(
            Request::Snapshot {
                root: self.root.clone(),
                directories,
                search,
            },
            cx,
        )
    }

    pub fn same_workspace(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.submit, &other.submit)
    }

    /// A machine that answers each request with `reply`, at once.
    #[cfg(any(test, feature = "test-support"))]
    pub fn fake(root: PathBuf, reply: impl Fn(Request) -> io::Result<Reply> + 'static) -> Self {
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

pub async fn receive(answer: Answer) -> io::Result<Reply> {
    answer
        .await
        .map_err(|_| io::Error::other("The remote file operation was cancelled."))?
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use gpui::TestAppContext;

    use super::*;

    /// Each operation names the checkout's root and what it was asked, and
    /// its answer is the machine's.
    #[gpui::test]
    async fn each_operation_asks_the_machine_about_its_checkout(cx: &mut TestAppContext) {
        let root = PathBuf::from("/work/app");
        let asked = Rc::new(RefCell::new(Vec::new()));
        let heard = asked.clone();
        let remote = RemoteFiles::fake(root.clone(), move |request| {
            heard.borrow_mut().push(request);
            Ok(Reply::Binary)
        });
        let file = root.join("src/main.rs");
        let answers = cx.update(|cx| {
            [
                remote.read(&file, cx),
                remote.write(&file, "fn main() {}".into(), None, cx),
                remote.snapshot(vec![root.join("src")], true, cx),
            ]
        });
        for answer in answers {
            assert!(matches!(receive(answer).await, Ok(Reply::Binary)));
        }
        assert_eq!(
            *asked.borrow(),
            [
                Request::Read {
                    root: root.clone(),
                    path: file.clone(),
                },
                Request::Write {
                    root: root.clone(),
                    path: file,
                    text: "fn main() {}".into(),
                    expected: None,
                },
                Request::Snapshot {
                    root: root.clone(),
                    directories: vec![root.join("src")],
                    search: true,
                },
            ]
        );
    }

    /// A copy reaches the same machine; another made over the same root
    /// does not.
    #[test]
    fn a_copy_is_the_same_workspace_and_another_is_not() {
        let root = PathBuf::from("/work/app");
        let remote = RemoteFiles::fake(root.clone(), |_| Ok(Reply::Binary));
        assert!(remote.same_workspace(&remote.clone()));
        assert!(!remote.same_workspace(&RemoteFiles::fake(root, |_| Ok(Reply::Binary))));
    }

    /// An answer dropped unsent says the operation was cancelled.
    #[gpui::test]
    async fn an_answer_never_sent_is_cancelled(_: &mut TestAppContext) {
        let (send, answer) = oneshot::channel();
        drop(send);
        let error = receive(answer).await.err().expect("no answer");
        assert_eq!(error.to_string(), "The remote file operation was cancelled.");
    }
}
