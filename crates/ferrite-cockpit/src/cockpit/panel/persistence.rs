//! One ordered writer for the Panel's tabs and Files working sets.

use std::io;
use std::path::Path;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::super::surfaces::Surfaces;
use super::super::tools::WorkingSets;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Revision {
    surfaces: u64,
    working_sets: u64,
}

impl Revision {
    fn of(surfaces: &Surfaces, working_sets: &WorkingSets) -> Self {
        Self {
            surfaces: surfaces.revision(),
            working_sets: working_sets.revision(),
        }
    }
}

struct Snapshot {
    revision: Revision,
    surfaces: Option<Surfaces>,
    working_sets: Option<WorkingSets>,
}

impl Snapshot {
    fn write(&self, dir: &Path) -> io::Result<()> {
        if let Some(surfaces) = &self.surfaces {
            surfaces.save(dir)?;
        }
        if let Some(working_sets) = &self.working_sets {
            working_sets.save(dir)?;
        }
        Ok(())
    }
}

type Writer = Arc<dyn Fn(&Snapshot, &Path) -> io::Result<()> + Send + Sync>;

struct Writing {
    revision: Revision,
    task: JoinHandle<io::Result<()>>,
}

impl Writing {
    fn join(self) -> (Revision, io::Result<()>) {
        let result = self
            .task
            .join()
            .unwrap_or_else(|_| Err(io::Error::other("the Panel writer panicked")));
        (self.revision, result)
    }
}

pub(super) struct Persistence {
    saved: Revision,
    writing: Option<Writing>,
    failures: u32,
    retry_at: Option<Instant>,
    writer: Option<Writer>,
}

impl Default for Persistence {
    fn default() -> Self {
        Self {
            saved: Revision::default(),
            writing: None,
            failures: 0,
            retry_at: None,
            writer: Some(Arc::new(Snapshot::write)),
        }
    }
}

impl Persistence {
    pub(super) fn retired() -> Self {
        Self {
            writer: None,
            ..Self::default()
        }
    }

    pub(super) fn surfaces_pending(&self, surfaces: &Surfaces) -> bool {
        surfaces.revision() != self.saved.surfaces
    }

    fn snapshot(&self, surfaces: &Surfaces, working_sets: &WorkingSets) -> Option<Snapshot> {
        let revision = Revision::of(surfaces, working_sets);
        (revision != self.saved).then(|| Snapshot {
            revision,
            surfaces: (revision.surfaces != self.saved.surfaces).then(|| surfaces.clone()),
            working_sets: (revision.working_sets != self.saved.working_sets)
                .then(|| working_sets.clone()),
        })
    }

    fn completed(&mut self, revision: Revision, result: io::Result<()>, now: Instant) {
        match result {
            Ok(()) => {
                self.saved = revision;
                self.failures = 0;
                self.retry_at = None;
            }
            Err(error) => {
                self.failures = self.failures.saturating_add(1);
                let seconds = (1_u64 << self.failures.saturating_sub(1).min(5)).min(30);
                self.retry_at = Some(now + Duration::from_secs(seconds));
                eprintln!("ferrite: the Panel was not saved, retrying in {seconds}s: {error}");
            }
        }
    }

    pub(super) fn pump(
        &mut self,
        dir: &Path,
        surfaces: &Surfaces,
        working_sets: &WorkingSets,
        now: Instant,
    ) {
        if self.writer.is_none() {
            return;
        }
        if self
            .writing
            .as_ref()
            .is_some_and(|writing| writing.task.is_finished())
        {
            let (revision, result) = self.writing.take().unwrap().join();
            self.completed(revision, result, now);
        }
        if self.writing.is_some() || self.retry_at.is_some_and(|at| now < at) {
            return;
        }
        let Some(snapshot) = self.snapshot(surfaces, working_sets) else {
            return;
        };
        let revision = snapshot.revision;
        let writer = self.writer.as_ref().unwrap().clone();
        let dir = dir.to_path_buf();
        match std::thread::Builder::new()
            .name("panel-save".into())
            .spawn(move || writer(&snapshot, &dir))
        {
            Ok(task) => self.writing = Some(Writing { revision, task }),
            Err(error) => self.completed(revision, Err(error), now),
        }
    }

    /// The last snapshot waits for the previous write, so quit cannot let an
    /// older writer replace the latest state after it has been flushed.
    pub(super) fn flush(
        &mut self,
        dir: &Path,
        surfaces: &Surfaces,
        working_sets: &WorkingSets,
    ) -> io::Result<()> {
        let Some(writer) = self.writer.clone() else {
            return Ok(());
        };
        if let Some(writing) = self.writing.take() {
            let (revision, result) = writing.join();
            self.completed(revision, result, Instant::now());
        }
        if let Some(snapshot) = self.snapshot(surfaces, working_sets) {
            writer(&snapshot, dir)?;
            self.completed(snapshot.revision, Ok(()), Instant::now());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cockpit::surfaces::{SurfaceAction, SurfaceId};
    use ferrite_core::ThreadId;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{mpsc, Mutex};

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ferrite-panel-persistence-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn open(surfaces: &mut Surfaces, id: SurfaceId) {
        surfaces.apply(ThreadId::new(7), true, SurfaceAction::Open(id));
    }

    fn active(dir: &Path) -> Option<SurfaceId> {
        Surfaces::load(dir, |_, _| true)
            .unwrap()
            .get(ThreadId::new(7))
            .and_then(|state| state.active.clone())
    }

    fn wait_for_write(persistence: &Persistence) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while persistence
            .writing
            .as_ref()
            .is_some_and(|writing| !writing.task.is_finished())
        {
            assert!(
                Instant::now() < deadline,
                "the background writer must finish"
            );
            std::thread::yield_now();
        }
    }

    fn settle(
        persistence: &mut Persistence,
        dir: &Path,
        surfaces: &Surfaces,
        files: &WorkingSets,
        now: Instant,
    ) {
        wait_for_write(persistence);
        persistence.pump(dir, surfaces, files, now);
    }

    #[test]
    fn failed_save_retries_without_another_edit() {
        let dir = scratch("retry");
        let calls = Arc::new(AtomicUsize::new(0));
        let attempts = calls.clone();
        let mut persistence = Persistence {
            writer: Some(Arc::new(move |snapshot, dir| {
                if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                    return Err(io::Error::other("disk unavailable"));
                }
                snapshot.write(dir)
            })),
            ..Default::default()
        };
        let mut surfaces = Surfaces::default();
        let files = WorkingSets::default();
        open(&mut surfaces, SurfaceId::Files);
        let now = Instant::now();
        persistence.pump(&dir, &surfaces, &files, now);
        settle(&mut persistence, &dir, &surfaces, &files, now);
        persistence.pump(&dir, &surfaces, &files, now);
        assert_eq!(calls.load(Ordering::SeqCst), 1, "failure backs off");
        assert_eq!(active(&dir), None);
        persistence.pump(&dir, &surfaces, &files, now + Duration::from_secs(2));
        settle(&mut persistence, &dir, &surfaces, &files, Instant::now());
        assert_eq!(active(&dir), Some(SurfaceId::Files));
        persistence.pump(&dir, &surfaces, &files, now + Duration::from_secs(60));
        assert_eq!(calls.load(Ordering::SeqCst), 2, "success stays saved");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn repeated_failures_back_off_to_a_thirty_second_limit() {
        let dir = scratch("backoff");
        let calls = Arc::new(AtomicUsize::new(0));
        let attempts = calls.clone();
        let mut persistence = Persistence {
            writer: Some(Arc::new(move |_, _| {
                attempts.fetch_add(1, Ordering::SeqCst);
                Err(io::Error::other("disk unavailable"))
            })),
            ..Default::default()
        };
        let mut surfaces = Surfaces::default();
        let files = WorkingSets::default();
        open(&mut surfaces, SurfaceId::Files);
        let mut now = Instant::now();
        persistence.pump(&dir, &surfaces, &files, now);
        settle(&mut persistence, &dir, &surfaces, &files, now);
        for (index, seconds) in [1, 2, 4, 8, 16, 30, 30].into_iter().enumerate() {
            now += Duration::from_secs(seconds);
            persistence.pump(&dir, &surfaces, &files, now - Duration::from_millis(1));
            assert_eq!(calls.load(Ordering::SeqCst), index + 1);
            persistence.pump(&dir, &surfaces, &files, now);
            settle(&mut persistence, &dir, &surfaces, &files, now);
            assert_eq!(calls.load(Ordering::SeqCst), index + 2);
        }
        assert!(!dir.exists());
    }

    #[test]
    fn an_older_write_cannot_clean_a_newer_edit() {
        let dir = scratch("newer");
        let (started, receiving) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let gate = Mutex::new(gate);
        let count = AtomicUsize::new(0);
        let mut persistence = Persistence {
            writer: Some(Arc::new(move |snapshot, dir| {
                if count.fetch_add(1, Ordering::SeqCst) == 0 {
                    started.send(()).unwrap();
                    gate.lock().unwrap().recv().unwrap();
                }
                snapshot.write(dir)
            })),
            ..Default::default()
        };
        let mut surfaces = Surfaces::default();
        let files = WorkingSets::default();
        open(&mut surfaces, SurfaceId::Files);
        persistence.pump(&dir, &surfaces, &files, Instant::now());
        receiving.recv_timeout(Duration::from_secs(2)).unwrap();
        open(&mut surfaces, SurfaceId::Changes);
        persistence.pump(&dir, &surfaces, &files, Instant::now());
        assert_eq!(
            active(&dir),
            None,
            "the newer write waits for the older one"
        );
        release.send(()).unwrap();
        wait_for_write(&persistence);
        assert_eq!(active(&dir), Some(SurfaceId::Files));
        persistence.pump(&dir, &surfaces, &files, Instant::now());
        settle(&mut persistence, &dir, &surfaces, &files, Instant::now());
        assert_eq!(active(&dir), Some(SurfaceId::Changes));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn transferring_the_panel_keeps_one_writer_and_the_latest_pending_state() {
        let dir = scratch("successor");
        let (started, receiving) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let gate = Mutex::new(gate);
        let calls = Arc::new(AtomicUsize::new(0));
        let attempts = calls.clone();
        let mut panel = super::super::Panel::load(&dir);
        panel.persistence.writer = Some(Arc::new(move |snapshot, dir| {
            if attempts.fetch_add(1, Ordering::SeqCst) == 0 {
                started.send(()).unwrap();
                gate.lock().unwrap().recv().unwrap();
            }
            snapshot.write(dir)
        }));
        open(&mut panel.surfaces, SurfaceId::Files);
        panel
            .persistence
            .pump(&dir, &panel.surfaces, &panel.working_sets, Instant::now());
        receiving.recv_timeout(Duration::from_secs(2)).unwrap();
        open(&mut panel.surfaces, SurfaceId::Changes);
        let mut successor = panel.successor();
        open(&mut panel.surfaces, SurfaceId::Overview);
        panel
            .persistence
            .pump(&dir, &panel.surfaces, &panel.working_sets, Instant::now());
        panel
            .persistence
            .flush(&dir, &panel.surfaces, &panel.working_sets)
            .unwrap();
        successor.persistence.pump(
            &dir,
            &successor.surfaces,
            &successor.working_sets,
            Instant::now(),
        );
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "the successor owns the existing writer"
        );
        release.send(()).unwrap();
        successor
            .persistence
            .flush(&dir, &successor.surfaces, &successor.working_sets)
            .unwrap();
        panel
            .persistence
            .flush(&dir, &panel.surfaces, &panel.working_sets)
            .unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(active(&dir), Some(SurfaceId::Changes));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn shutdown_waits_for_the_previous_snapshot_before_flushing_the_latest() {
        let dir = scratch("quit");
        let (started, receiving) = mpsc::channel();
        let (release, gate) = mpsc::channel();
        let gate = Mutex::new(gate);
        let count = AtomicUsize::new(0);
        let mut persistence = Persistence {
            writer: Some(Arc::new(move |snapshot, dir| {
                if count.fetch_add(1, Ordering::SeqCst) == 0 {
                    started.send(()).unwrap();
                    gate.lock().unwrap().recv().unwrap();
                }
                snapshot.write(dir)
            })),
            ..Default::default()
        };
        let mut surfaces = Surfaces::default();
        let files = WorkingSets::default();
        open(&mut surfaces, SurfaceId::Files);
        persistence.pump(&dir, &surfaces, &files, Instant::now());
        receiving.recv_timeout(Duration::from_secs(2)).unwrap();
        open(&mut surfaces, SurfaceId::Changes);
        let saved_dir = dir.clone();
        let flushed = std::thread::spawn(move || persistence.flush(&saved_dir, &surfaces, &files));
        release.send(()).unwrap();
        flushed.join().unwrap().unwrap();
        assert_eq!(active(&dir), Some(SurfaceId::Changes));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
