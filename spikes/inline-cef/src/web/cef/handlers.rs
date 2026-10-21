//! The CEF callbacks.
//!
//! Threads: the app/browser-process, render, display, life-span, context-menu
//! and dialog handlers, and `RequestHandler::on_before_browse`, run on the
//! browser process's UI thread (= the host's main thread, since the message
//! loop is pumped from there). `on_schedule_message_pump_work` may run on any
//! thread. The resource-request and resource handlers run on CEF's IO thread.
//! So everything here shares state through `Arc` + atomics/mutexes, and never
//! calls into the host: frames and updates are parked in [`ViewState`] and the
//! engine wakes the host from `pump()`.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use cef::*;

use super::{document, input, Global, PageDoc, ViewState, MAX_HEIGHT, SCHEME};
use crate::web::Frame as WebFrame;

// ---------------------------------------------------------------------------
// App (every process) and the browser-process handler.

wrap_app! {
    pub(super) struct InlineApp {
        global: Arc<Global>,
    }

    impl App {
        fn on_before_command_line_processing(
            &self,
            process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            let is_browser = process_type.map(|t| t.to_string().is_empty()).unwrap_or(true);
            let Some(command_line) = command_line else { return };
            if !is_browser {
                return;
            }
            for switch in [
                // No windows, dialogs, keychain prompts or first-run UI of its own.
                "no-startup-window",
                "noerrdialogs",
                "hide-crash-restore-bubble",
                "use-mock-keychain",
                "no-first-run",
                // No traffic Chromium would make on its own behalf.
                "disable-background-networking",
                "disable-component-update",
                "disable-sync",
                "disable-default-apps",
                "disable-extensions",
            ] {
                command_line.append_switch(Some(&switch.into()));
            }
            // Fewer processes (measured in NOTES-cef.md): no pre-warmed spare
            // renderer, and the network service (which only ever serves our
            // in-memory page) in the browser process.
            command_line.append_switch_with_value(
                Some(&"disable-features".into()),
                Some(&"SpareRendererForSitePerProcess".into()),
            );
            command_line.append_switch_with_value(
                Some(&"enable-features".into()),
                Some(&"NetworkServiceInProcess2".into()),
            );
            // Every page is on the one site `ferrite-view://page`, so this
            // puts all views in a single renderer process instead of one each
            // (~35 MB apiece). Trade-off in NOTES-cef.md ("Memory").
            // INLINE_CEF_PROCESS_PER_VIEW=1 restores Chromium's default.
            if std::env::var_os("INLINE_CEF_PROCESS_PER_VIEW").is_none() {
                command_line.append_switch(Some(&"process-per-site".into()));
            }
            // For experiments, e.g. "disable-gpu disable-gpu-compositing"
            // (software compositing: ~65 MB less, no WebGL).
            if let Ok(extra) = std::env::var("INLINE_CEF_EXTRA_SWITCHES") {
                for s in extra.split_whitespace() {
                    match s.split_once('=') {
                        Some((k, v)) => command_line.append_switch_with_value(Some(&k.into()), Some(&v.into())),
                        None => command_line.append_switch(Some(&s.into())),
                    }
                }
            }
        }

        /// Runs in every process, so the scheme means the same everywhere.
        fn on_register_custom_schemes(&self, registrar: Option<&mut SchemeRegistrar>) {
            let Some(registrar) = registrar else { return };
            // STANDARD: a real origin (relative URLs, `location`, storage).
            // SECURE: a secure context, like https, so modern APIs work.
            let options = SchemeOptions::STANDARD.get_raw() | SchemeOptions::SECURE.get_raw();
            registrar.add_custom_scheme(Some(&SCHEME.into()), options as i32);
        }

        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(ProcessHandler::new(self.global.clone()))
        }
    }
}

wrap_browser_process_handler! {
    struct ProcessHandler {
        global: Arc<Global>,
    }

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            self.global.context_ready.store(true, Ordering::SeqCst);
        }

        /// External message pump: CEF asks for `do_message_loop_work` in
        /// `delay_ms`. Any thread. `pump()` turns this into its return value.
        fn on_schedule_message_pump_work(&self, delay_ms: i64) {
            self.global.schedule(delay_ms);
        }
    }
}

// ---------------------------------------------------------------------------
// Per-view client.

/// The handlers for one browser of a view; `generation` is that browser's.
pub(super) fn client(state: Arc<ViewState>, page: Arc<PageDoc>, global: Arc<Global>, generation: u64) -> Client {
    ViewClient::new(
        ViewRender::new(state.clone(), generation),
        ViewDisplay::new(state, generation),
        ViewLifeSpan::new(global),
        ViewRequests::new(page),
        NoContextMenu::new(),
        NoDialogs::new(),
    )
}

wrap_client! {
    struct ViewClient {
        render: RenderHandler,
        display: DisplayHandler,
        life_span: LifeSpanHandler,
        requests: RequestHandler,
        context_menu: ContextMenuHandler,
        dialogs: JsdialogHandler,
    }

    impl Client {
        fn render_handler(&self) -> Option<RenderHandler> {
            Some(self.render.clone())
        }
        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(self.display.clone())
        }
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(self.life_span.clone())
        }
        fn request_handler(&self) -> Option<RequestHandler> {
            Some(self.requests.clone())
        }
        fn context_menu_handler(&self) -> Option<ContextMenuHandler> {
            Some(self.context_menu.clone())
        }
        fn jsdialog_handler(&self) -> Option<JsdialogHandler> {
            Some(self.dialogs.clone())
        }
    }
}

wrap_render_handler! {
    struct ViewRender {
        state: Arc<ViewState>,
        generation: u64,
    }

    impl RenderHandler {
        /// The view's size in DIPs: the host's width, the page's height.
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            let Some(rect) = rect else { return };
            let geometry = self.state.geometry();
            *rect = Rect { x: 0, y: 0, width: geometry.width as i32, height: geometry.height as i32 };
        }

        fn screen_info(
            &self,
            _browser: Option<&mut Browser>,
            screen_info: Option<&mut ScreenInfo>,
        ) -> ::std::os::raw::c_int {
            let Some(screen_info) = screen_info else { return 0 };
            let geometry = self.state.geometry();
            let rect = Rect { x: 0, y: 0, width: geometry.width as i32, height: geometry.height as i32 };
            screen_info.device_scale_factor = geometry.scale;
            screen_info.depth = 24;
            screen_info.depth_per_component = 8;
            screen_info.rect = rect.clone();
            screen_info.available_rect = rect;
            1
        }

        /// The CPU path: CEF hands over the whole view as BGRA in device
        /// pixels. Copy it into the view's slot; a newer frame replaces an
        /// untaken one. `<select>` popups (PET_POPUP) are not composited yet.
        fn on_paint(
            &self,
            _browser: Option<&mut Browser>,
            type_: PaintElementType,
            _dirty_rects: Option<&[Rect]>,
            buffer: *const u8,
            width: ::std::os::raw::c_int,
            height: ::std::os::raw::c_int,
        ) {
            if type_ != PaintElementType::VIEW || buffer.is_null() || width <= 0 || height <= 0 {
                return;
            }
            // A closing browser, or a page that hasn't laid itself out yet:
            // the host keeps the frame it has.
            if !self.state.is_current(self.generation) || !self.state.loaded.load(Ordering::SeqCst) {
                return;
            }
            if !self.state.visible.load(Ordering::SeqCst) {
                if self.state.trace {
                    eprintln!("inline-cef: dropped a paint while hidden");
                }
                return;
            }
            let len = width as usize * height as usize * 4;
            // SAFETY: CEF guarantees `width * height * 4` bytes for the call.
            let bgra = unsafe { std::slice::from_raw_parts(buffer, len) }.to_vec();
            self.state.update(|updates| {
                updates.frame = Some(WebFrame { width: width as u32, height: height as u32, bgra });
            });
            self.state.restyle_unpainted.store(false, Ordering::SeqCst);
        }
    }
}

wrap_display_handler! {
    struct ViewDisplay {
        state: Arc<ViewState>,
        generation: u64,
    }

    impl DisplayHandler {
        /// Carries the host script's height reports (see `document`).
        fn on_console_message(
            &self,
            browser: Option<&mut Browser>,
            _level: LogSeverity,
            message: Option<&CefString>,
            _source: Option<&CefString>,
            _line: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            let Some(height) = message.and_then(|m| document::parse_height(&m.to_string())) else {
                return 0;
            };
            if !self.state.is_current(self.generation) {
                return 1;
            }
            let height = height.clamp(1, MAX_HEIGHT);
            let first = !self.state.loaded.swap(true, Ordering::SeqCst);
            let resized = self.state.set_content_height(height);
            if let Some(host) = browser.and_then(|b| b.host()) {
                if resized {
                    host.was_resized();
                } else if first {
                    // Frames painted before this were dropped; ask for one of
                    // the laid-out page (a resize would have brought one).
                    host.invalidate(PaintElementType::VIEW);
                }
            }
            1
        }

        fn on_cursor_change(
            &self,
            _browser: Option<&mut Browser>,
            _cursor: *mut u8,
            type_: CursorType,
            _custom_cursor_info: Option<&CursorInfo>,
        ) -> ::std::os::raw::c_int {
            if self.state.is_current(self.generation) {
                self.state.set_cursor(input::cursor(type_));
            }
            1
        }

        /// Off-screen there is no native tooltip; swallow rather than let CEF try.
        fn on_tooltip(
            &self,
            _browser: Option<&mut Browser>,
            _text: Option<&mut CefString>,
        ) -> ::std::os::raw::c_int {
            1
        }
    }
}

wrap_life_span_handler! {
    struct ViewLifeSpan {
        global: Arc<Global>,
    }

    impl LifeSpanHandler {
        /// No popups, ever (`window.open`, `target=_blank`).
        fn on_before_popup(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: ::std::os::raw::c_int,
            _target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: ::std::os::raw::c_int,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            _no_javascript_access: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            1
        }

        fn on_after_created(&self, _browser: Option<&mut Browser>) {
            self.global.live_browsers.fetch_add(1, Ordering::SeqCst);
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            self.global.live_browsers.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

wrap_request_handler! {
    struct ViewRequests {
        page: Arc<PageDoc>,
    }

    impl RequestHandler {
        /// No navigation away from the page (links, `location = …`, forms),
        /// in the main frame or any subframe.
        fn on_before_browse(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _user_gesture: ::std::os::raw::c_int,
            _is_redirect: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            let url = request.map(|r| CefString::from(&r.url()).to_string()).unwrap_or_default();
            (without_fragment(&url) != self.page.url) as i32
        }

        fn on_open_urlfrom_tab(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _target_url: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            1
        }

        fn resource_request_handler(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            _is_navigation: ::std::os::raw::c_int,
            _is_download: ::std::os::raw::c_int,
            _request_initiator: Option<&CefString>,
            _disable_default_handling: Option<&mut ::std::os::raw::c_int>,
        ) -> Option<ResourceRequestHandler> {
            Some(PageRequests::new(self.page.clone()))
        }
    }
}

// ---------------------------------------------------------------------------
// IO thread: every request the page makes. Serve the document, refuse the rest.

wrap_resource_request_handler! {
    struct PageRequests {
        page: Arc<PageDoc>,
    }

    impl ResourceRequestHandler {
        fn on_before_resource_load(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _callback: Option<&mut Callback>,
        ) -> ReturnValue {
            let url = request.map(|r| CefString::from(&r.url()).to_string()).unwrap_or_default();
            let local = url.starts_with("data:") || url.starts_with("blob:");
            if local || without_fragment(&url) == self.page.url {
                ReturnValue::CONTINUE
            } else {
                ReturnValue::CANCEL
            }
        }

        fn resource_handler(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
        ) -> Option<ResourceHandler> {
            let url = request.map(|r| CefString::from(&r.url()).to_string()).unwrap_or_default();
            if without_fragment(&url) != self.page.url {
                return None;
            }
            let bytes: Arc<[u8]> = self.page.document().into_bytes().into();
            Some(PageResource::new(bytes, Arc::new(Mutex::new(0))))
        }
    }
}

wrap_resource_handler! {
    struct PageResource {
        bytes: Arc<[u8]>,
        offset: Arc<Mutex<usize>>,
    }

    impl ResourceHandler {
        fn open(
            &self,
            _request: Option<&mut Request>,
            handle_request: Option<&mut ::std::os::raw::c_int>,
            _callback: Option<&mut Callback>,
        ) -> ::std::os::raw::c_int {
            if let Some(handle_request) = handle_request {
                *handle_request = 1;
            }
            1
        }

        fn response_headers(
            &self,
            response: Option<&mut Response>,
            response_length: Option<&mut i64>,
            _redirect_url: Option<&mut CefString>,
        ) {
            if let Some(response) = response {
                response.set_status(200);
                response.set_status_text(Some(&"OK".into()));
                response.set_mime_type(Some(&"text/html".into()));
                response.set_charset(Some(&"utf-8".into()));
                response.set_header_by_name(
                    Some(&"Content-Security-Policy".into()),
                    Some(&document::CSP.into()),
                    1,
                );
                response.set_header_by_name(Some(&"Cache-Control".into()), Some(&"no-store".into()), 1);
            }
            if let Some(length) = response_length {
                *length = self.bytes.len() as i64;
            }
        }

        fn read(
            &self,
            data_out: *mut u8,
            bytes_to_read: ::std::os::raw::c_int,
            bytes_read: Option<&mut ::std::os::raw::c_int>,
            _callback: Option<&mut ResourceReadCallback>,
        ) -> ::std::os::raw::c_int {
            let mut offset = self.offset.lock().unwrap();
            let rest = &self.bytes[*offset..];
            let n = rest.len().min(bytes_to_read.max(0) as usize);
            if n > 0 {
                // SAFETY: CEF provides `bytes_to_read` writable bytes at `data_out`.
                unsafe { std::ptr::copy_nonoverlapping(rest.as_ptr(), data_out, n) };
            }
            *offset += n;
            if let Some(bytes_read) = bytes_read {
                *bytes_read = n as i32;
            }
            (n > 0) as i32
        }

        fn cancel(&self) {}
    }
}

// ---------------------------------------------------------------------------
// Native UI the page must not summon off-screen.

wrap_context_menu_handler! {
    struct NoContextMenu;

    impl ContextMenuHandler {
        fn on_before_context_menu(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _params: Option<&mut ContextMenuParams>,
            model: Option<&mut MenuModel>,
        ) {
            if let Some(model) = model {
                model.clear();
            }
        }
    }
}

wrap_jsdialog_handler! {
    struct NoDialogs;

    impl JsdialogHandler {
        /// `alert`/`confirm`/`prompt`: suppressed (as if dismissed).
        fn on_jsdialog(
            &self,
            _browser: Option<&mut Browser>,
            _origin_url: Option<&CefString>,
            _dialog_type: JsdialogType,
            _message_text: Option<&CefString>,
            _default_prompt_text: Option<&CefString>,
            _callback: Option<&mut JsdialogCallback>,
            suppress_message: Option<&mut ::std::os::raw::c_int>,
        ) -> ::std::os::raw::c_int {
            if let Some(suppress_message) = suppress_message {
                *suppress_message = 1;
            }
            0
        }

        fn on_before_unload_dialog(
            &self,
            _browser: Option<&mut Browser>,
            _message_text: Option<&CefString>,
            _is_reload: ::std::os::raw::c_int,
            callback: Option<&mut JsdialogCallback>,
        ) -> ::std::os::raw::c_int {
            if let Some(callback) = callback {
                callback.cont(1, None);
            }
            1
        }
    }
}

fn without_fragment(url: &str) -> &str {
    url.split('#').next().unwrap_or(url)
}
