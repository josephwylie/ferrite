//! What a Composer's files are bound for, and whether a send may take them.
//! A file that cannot be sent stays in the draft, marked in its own place in
//! the tray (`ferrite_core::attachment`), until it is repaired or removed.

use super::*;
use ferrite_core::attachment::Destination;
use ferrite_pane::composer::Composer;
use std::path::PathBuf;

impl CockpitView {
    /// The machine pane `index`'s agent works on: its Thread's, or its
    /// draft's Project's.
    pub(super) fn pane_machine(&self, index: usize, cx: &gpui::App) -> Option<MachineId> {
        let pane = self.panes.get(index)?;
        match (pane.thread(), pane.read(cx).draft()) {
            (Some(thread), _) => Some(thread.machine()),
            (None, Some(draft)) => Some(draft.binding.project().machine()),
            _ => None,
        }
    }

    /// `composer`'s files are going to an agent on `machine`: this
    /// computer or another, and kept in the app's own store.
    pub(super) fn aim_attachments(
        &self,
        composer: &Entity<Composer>,
        machine: MachineId,
        cx: &mut Context<Self>,
    ) {
        let destination = if self.elsewhere(machine, cx) {
            Destination::Remote
        } else {
            Destination::Local
        };
        let store = self.prefs.dir.clone();
        composer.update(cx, |composer, cx| {
            composer.aim_files(destination, store, cx)
        });
    }

    /// Whether pane `index`'s send may go ahead. Its files are looked at as
    /// they are now, so one repaired since it failed passes; one that still
    /// fails stops the send and says why in the tray. The files of this
    /// computer that pass are replaced by retained copies, so what is sent is
    /// what is kept; a file inside the pane's workspace, however it was
    /// attached, is sent as its own path.
    pub(super) fn admit_files(&mut self, index: usize, cx: &mut Context<Self>) -> bool {
        let Some(pane) = self.panes.get(index) else {
            return false;
        };
        let composer = pane.read(cx).composer.clone();
        if let Some(machine) = self.pane_machine(index, cx) {
            self.upload_attachments(&composer, machine, cx);
        }
        if composer.update(cx, |composer, cx| composer.has_blocked_file(cx)) {
            return false;
        }
        let root = self.pane_root(index, cx);
        composer.update(cx, |composer, cx| {
            composer.retain_files(root.as_deref(), cx)
        })
    }

    #[cfg(feature = "headless")]
    pub(crate) fn attach_local_files(
        &mut self,
        paths: &[PathBuf],
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let pane = self
            .panes
            .get(self.focused(cx))
            .ok_or("Open a Thread or draft before attaching files.")?;
        pane.composer
            .update(cx, |composer, cx| composer.add_local_files(paths, cx));
        Ok(())
    }

    /// Send `composer`'s files still on this computer to `machine`, when
    /// that is another machine.
    pub(super) fn upload_attachments(
        &mut self,
        composer: &Entity<Composer>,
        machine: MachineId,
        cx: &mut Context<Self>,
    ) {
        let Some(bridge) = self.cockpit.read(cx).bridge(machine).cloned() else {
            return;
        };
        for (path, attempt) in composer.update(cx, |composer, cx| composer.pending_uploads(cx)) {
            let upload = match self
                .cockpit
                .update(cx, |cockpit, _| cockpit.upload_attachment(machine, path.clone()))
            {
                Ok(upload) => upload,
                Err(error) => {
                    composer.update(cx, |composer, cx| {
                        composer.finish_upload(&path, attempt, Err(error.to_string()), cx);
                    });
                    continue;
                }
            };
            let composer = composer.downgrade();
            let bridge = bridge.clone();
            cx.spawn(async move |this, cx| {
                let _cancel = CancelUpload(upload.cancel);
                let started = cx.background_executor().now();
                loop {
                    let active = this
                        .update(cx, |view, cx| {
                            view.same_bridge(machine, &bridge, cx)
                                && composer
                                    .update(cx, |composer, _| {
                                        composer.uploading_file(&path, attempt)
                                    })
                                    .unwrap_or(false)
                        })
                        .unwrap_or(false);
                    if !active {
                        return;
                    }
                    if let Some(progress) = upload.progress.try_iter().last() {
                        let _ = composer.update(cx, |composer, cx| {
                            composer.upload_progress(
                                &path,
                                attempt,
                                progress.sent,
                                progress.total,
                                cx,
                            );
                        });
                    }
                    let result = match upload.result.try_recv() {
                        Ok(result) => Some(
                            result
                                .map(|receipt| receipt.path)
                                .map_err(|error| error.to_string()),
                        ),
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err(
                            "The connection closed before the upload finished.".into(),
                        )),
                        Err(std::sync::mpsc::TryRecvError::Empty)
                            if cx.background_executor().now().duration_since(started)
                                >= Duration::from_secs(300) =>
                        {
                            Some(Err("The upload timed out.".into()))
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => None,
                    };
                    if let Some(result) = result {
                        let _ =
                            this.update(cx, |view, cx| {
                                if !view.same_bridge(machine, &bridge, cx) {
                                    return;
                                }
                                let remote = result.as_ref().ok().cloned();
                                let accepted = composer
                                    .update(cx, |composer, cx| {
                                        composer.finish_upload(&path, attempt, result, cx)
                                    })
                                    .unwrap_or(false);
                                if accepted {
                                    if let Some(remote) = remote {
                                        if let Some(pane) = view
                                            .panes
                                            .iter()
                                            .find(|pane| pane.read(cx).composer.downgrade() == composer)
                                        {
                                            pane.read(cx).preview.request_attachment(&remote);
                                        }
                                    }
                                    cx.notify();
                                }
                            });
                        return;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(30))
                        .await;
                }
            })
            .detach();
        }
    }

    /// Whether `machine` is still reached through `bridge`: an answer that
    /// came over a link since let go is no longer the window's.
    fn same_bridge(
        &self,
        machine: MachineId,
        bridge: &std::sync::Arc<ferrite_core::machine::Bridge>,
        cx: &gpui::App,
    ) -> bool {
        self.cockpit
            .read(cx)
            .bridge(machine)
            .is_some_and(|current| std::sync::Arc::ptr_eq(current, bridge))
    }

    /// Read the attachments each Pane's preview asks for from its
    /// machine, when that is another machine.
    pub(super) fn fetch_attachment_previews(&mut self, cx: &mut Context<Self>) {
        let mut requests = Vec::new();
        for index in 0..self.panes.len() {
            let Some(machine) = self.pane_machine(index, cx) else {
                continue;
            };
            let Some(bridge) = self.cockpit.read(cx).bridge(machine).cloned() else {
                continue;
            };
            let pane = &self.panes[index];
            for path in pane.read(cx).preview.take_attachment_requests() {
                requests.push((
                    machine,
                    bridge.clone(),
                    pane.read(cx).preview.clone(),
                    pane.read(cx).composer.entity_id(),
                    path,
                ));
            }
        }
        for (machine, bridge, preview, owner, (path, request)) in requests {
            let key = self
                .cockpit
                .read(cx)
                .saved_as(machine)
                .and_then(|key| key.0.clone())
                .unwrap_or_else(|| format!("session-{:p}", std::sync::Arc::as_ptr(&bridge)));
            let store =
                ferrite_core::machine::dir(&self.prefs.dir, &key).join("attachment-previews");
            let receiver = match self
                .cockpit
                .update(cx, |cockpit, _| cockpit.read_attachment(machine, &path))
            {
                Ok(receiver) => receiver,
                Err(error) => {
                    preview.finish_attachment(&path, request, Err(error.to_string()));
                    cx.notify();
                    continue;
                }
            };
            let store = ferrite_core::machine::dir(
                &store.join(format!("{owner:?}")),
                &path.to_string_lossy(),
            );
            let name = path.file_name().unwrap_or_default().to_owned();
            let executor = cx.background_executor().clone();
            let timer = executor.clone();
            let read = executor.spawn(async move {
                let started = timer.now();
                let bytes = loop {
                    match receiver.try_recv() {
                        Ok(result) => break result?,
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            return Err(std::io::Error::other(
                                "The connection closed before the preview arrived.",
                            ))
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {}
                    }
                    if timer.now().duration_since(started) >= Duration::from_secs(60) {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "The attachment preview timed out. Open it to retry.",
                        ));
                    }
                    timer.timer(Duration::from_millis(30)).await;
                };
                ferrite_core::attachment::retain_bytes(&bytes, &name, &store)
            });
            cx.spawn(async move |this, cx| {
                let result = read.await.map_err(|error| error.to_string());
                let _ = this.update(cx, |view, cx| {
                    if view.same_bridge(machine, &bridge, cx) {
                        preview.finish_attachment(&path, request, result);
                        cx.notify();
                    }
                });
            })
            .detach();
        }
    }

    /// The folder pane `index`'s agent works in: its Thread's, or the one
    /// its draft is bound to.
    pub(super) fn pane_root(&self, index: usize, cx: &gpui::App) -> Option<PathBuf> {
        let pane = self.panes.get(index)?;
        match (pane.thread(), pane.read(cx).draft()) {
            (Some(thread), _) => Some(
                self.cockpit
                    .read(cx)
                    .thread(thread)?
                    .workspace()?
                    .cwd()
                    .to_path_buf(),
            ),
            (None, Some(draft)) => {
                let machine = draft.binding.project().machine();
                Some(
                    draft
                        .binding
                        .resolve(
                            self.cockpit.read(cx).project(draft.binding.project()),
                            !self.elsewhere(machine, cx),
                        )
                        .ok()?
                        .source_root()
                        .to_path_buf(),
                )
            }
            _ => None,
        }
    }
}

struct CancelUpload(ferrite_core::chunked::Cancellation);

impl Drop for CancelUpload {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
