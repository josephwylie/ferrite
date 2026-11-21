//! One FIFO owns browser file work. Only completions return to the pump.
use super::*;
use ferrite_core::browser_annotations::Capture;

type Answer = mpsc::Sender<Result<Option<Done>, WireError>>;
pub(super) enum Job {
    Save(Store, ferrite_core::browser::Saved, Answer),
    #[cfg(test)]
    Hold(Receiver<()>, mpsc::Sender<()>),
    Capture(PathBuf, Capture, Option<Vec<u8>>, Answer),
    Image(PathBuf, String, Answer),
}

pub(crate) struct Disk {
    jobs: Arc<mpsc::Sender<Job>>,
    store: Store,
}

#[cfg(test)]
pub(crate) struct Control(std::sync::Weak<mpsc::Sender<Job>>);
impl Disk {
    pub(crate) fn new(store: Store) -> Self {
        let (send, jobs) = mpsc::channel();
        std::thread::spawn(move || {
            ferrite_core::spawn::serve_the_operator();
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
                        let result = ferrite_core::browser_annotations::retain_optional(
                            &dir,
                            capture,
                            bytes.as_deref(),
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
                            let bytes = ferrite_core::browser_annotations::read_retained(&dir, &hash)
                                .map_err(failed)?;
                            let (width, height) =
                                ferrite_core::browser_annotations::png_size(&bytes).map_err(failed)?;
                            Ok(Some(Done::BrowserCaptureImage {
                                png: ferrite_core::protocol::WirePng {
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
    pub(crate) fn control(&self) -> Control {
        Control(Arc::downgrade(&self.jobs))
    }
    pub(super) fn save(
        &self,
        saved: ferrite_core::browser::Saved,
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
    pub(crate) fn hold(&self) -> mpsc::Sender<()> {
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
