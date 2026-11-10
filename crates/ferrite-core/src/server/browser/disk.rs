//! One FIFO owns browser file work. Only completions return to the pump.
use super::*;
use crate::browser_annotations::Capture;

type Answer = mpsc::Sender<Result<Option<Done>, WireError>>;
pub(super) enum Job {
    Save(Store, crate::browser::Saved, Answer),
    #[cfg(test)]
    Hold(Receiver<()>, mpsc::Sender<()>),
    Capture(PathBuf, Capture, Vec<u8>, Answer),
    Image(PathBuf, String, Answer),
}

pub(in crate::server) struct Disk {
    jobs: Arc<mpsc::Sender<Job>>,
    store: Store,
}

#[cfg(test)]
pub(in crate::server) struct Control(std::sync::Weak<mpsc::Sender<Job>>);
impl Disk {
    pub(in crate::server) fn new(store: Store) -> Self {
        let (send, jobs) = mpsc::channel();
        std::thread::spawn(move || {
            crate::spawn::serve_the_operator();
            for job in jobs {
                match job {
                    #[cfg(test)]
                    Job::Hold(open, entered) => {
                        let _ = entered.send(());
                        let _ = open.recv();
                    }
                    Job::Save(store, saved, answer) => {
                        let result = store.save_browser(&saved).map(|_| None).map_err(failed);
                        drop(store);
                        let _ = answer.send(result);
                    }
                    Job::Capture(dir, capture, bytes, answer) => {
                        let result = crate::browser_annotations::retain(
                            &dir,
                            capture,
                            &bytes,
                            String::new(),
                            vec![],
                        )
                        .map(|annotation| Some(Done::BrowserCapture { annotation }))
                        .map_err(failed);
                        let _ = answer.send(result);
                    }
                    Job::Image(dir, hash, answer) => {
                        use base64::Engine as _;
                        let result = (|| {
                            let bytes = crate::browser_annotations::read_retained(&dir, &hash)
                                .map_err(failed)?;
                            let (width, height) =
                                crate::browser_annotations::png_size(&bytes).map_err(failed)?;
                            Ok(Some(Done::BrowserCaptureImage {
                                png: crate::protocol::WirePng {
                                    base64: base64::engine::general_purpose::STANDARD.encode(bytes),
                                    width,
                                    height,
                                },
                            }))
                        })();
                        let _ = answer.send(result);
                    }
                }
            }
        });
        Self {
            jobs: Arc::new(send),
            store,
        }
    }
    #[cfg(test)]
    pub(in crate::server) fn control(&self) -> Control {
        Control(Arc::downgrade(&self.jobs))
    }
    pub(super) fn save(
        &self,
        saved: crate::browser::Saved,
    ) -> Receiver<Result<Option<Done>, WireError>> {
        self.submit(|answer| Job::Save(self.store.clone(), saved, answer))
    }
    pub(super) fn submit(
        &self,
        job: impl FnOnce(Answer) -> Job,
    ) -> Receiver<Result<Option<Done>, WireError>> {
        let (answer, answered) = mpsc::channel();
        let _ = self.jobs.send(job(answer));
        answered
    }
}

#[cfg(test)]
impl Control {
    pub(in crate::server) fn hold(&self) -> mpsc::Sender<()> {
        let (open, wait) = mpsc::channel();
        let (entered, ready) = mpsc::channel();
        self.0
            .upgrade()
            .expect("browser worker available")
            .send(Job::Hold(wait, entered))
            .unwrap();
        ready.recv_timeout(Duration::from_secs(5)).unwrap();
        open
    }
}
