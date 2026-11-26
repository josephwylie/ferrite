//! Signing a Machine's CLI in from this computer (ADR 0051): the sheet that
//! walks one sign-in, which the Machine's Server runs with the CLI's own.
//! It opens the CLI's page in this computer's browser; for Claude the
//! operator pastes back the code the page shows, for Codex they enter the
//! code shown here on the page. The view owns where the sign-in stands, as
//! one state. What it asks of the Machine goes through its `Ask`, which the
//! Cockpit makes over that Machine's Server and a test over answers of its
//! own; it holds no window and no Server.

use std::io;
use std::rc::Rc;
use std::sync::mpsc::{Receiver, TryRecvError};

use ferrite_core::protocol::{Done, SignInStep, Withheld};
use ferrite_core::provider::Provider;
use ferrite_pane::composer::{Composer, Role};
use ferrite_ui::theme::*;
use gpui::prelude::*;
use gpui::{
    div, px, rgb, App, ClickEvent, Context, Div, Entity, EventEmitter, FocusHandle, Focusable, SharedString,
    Window,
};

gpui::actions!(sign_in, [Submit]);

/// The key context of the code's line (`ferrite_ui::keymap`): ↵ signs in.
const KEY_CONTEXT: &str = "SignIn";

/// Ask the Machine's Server one step of the sign-in, answered later.
pub type Ask = Rc<dyn Fn(SignInStep, &mut App) -> io::Result<Receiver<io::Result<Done>>>>;

type Answer = Receiver<io::Result<Done>>;

pub struct SignInView {
    provider: Provider,
    /// The Machine, as its name says it.
    place: SharedString,
    ask: Ask,
    flow: Flow,
    /// The code Claude's page shows, pasted.
    code: Entity<Composer>,
    focus: FocusHandle,
}

/// Where the sign-in stands.
enum Flow {
    /// `start` is out.
    Starting(Answer),
    /// The page is open in the browser; `finishing` is the `finish` out.
    Browser { page: Page, finishing: Option<Answer> },
    SignedIn,
    Failed(SharedString),
}

/// The CLI's page, as its Server said it.
struct Page {
    url: SharedString,
    user_code: Option<Withheld>,
    paste: bool,
}

/// What the sheet asks of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignInEvent {
    /// The CLI says it is signed in.
    SignedIn,
    /// Its Cancel, Close or Done: the sheet goes.
    Close,
}

impl EventEmitter<SignInEvent> for SignInView {}

impl SignInView {
    /// The sheet for `provider`'s CLI on the Machine `place` names, asking
    /// through `ask`: its sign-in starts at once.
    pub fn new(provider: Provider, place: SharedString, ask: Ask, cx: &mut Context<Self>) -> Self {
        let code = cx.new(Composer::new);
        code.update(cx, |code, cx| code.set_role(Role::Field(Some(KEY_CONTEXT)), cx));
        Self {
            provider,
            flow: started(&ask, &place, cx),
            place,
            ask,
            code,
            focus: cx.focus_handle(),
        }
    }

    /// What holds the keyboard while the sheet is up: the code's line while
    /// it waits for one, else the card.
    pub fn focus(&self, cx: &App) -> FocusHandle {
        match &self.flow {
            Flow::Browser {
                page: Page { paste: true, .. },
                finishing: None,
            } => self.code.read(cx).focus_handle(cx),
            _ => self.focus.clone(),
        }
    }

    /// Ask the Machine to start the sign-in, anew.
    fn start(&mut self, cx: &mut Context<Self>) {
        self.flow = started(&self.ask, &self.place, cx);
        cx.notify();
    }

    /// Ask the Machine to finish the sign-in with `code`.
    fn finish(&mut self, code: Option<Withheld>, cx: &mut Context<Self>) {
        match (self.ask)(SignInStep::Finish { code }, cx) {
            Ok(answer) => {
                if let Flow::Browser { finishing, .. } = &mut self.flow {
                    *finishing = Some(answer);
                }
            }
            Err(error) => self.flow = Flow::Failed(said(&self.place, &error)),
        }
        cx.notify();
    }

    /// The pasted code goes to the Machine, once there is one.
    fn submit(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.flow, Flow::Browser { page: Page { paste: true, .. }, finishing: None }) {
            return;
        }
        let code = self.code.read(cx).text().trim().to_string();
        if !code.is_empty() {
            self.finish(Some(Withheld::new(code)), cx);
        }
    }

    /// Take the Machine's answer, once it lands: the page opens in the
    /// browser, and Codex's sign-in is finished at once, its CLI waiting on
    /// the page's approval.
    pub fn poll(&mut self, cx: &mut Context<Self>) {
        let answer = match &self.flow {
            Flow::Starting(answer)
            | Flow::Browser {
                finishing: Some(answer),
                ..
            } => answer,
            _ => return,
        };
        let answer = match answer.try_recv() {
            Err(TryRecvError::Empty) => return,
            Ok(answer) => answer,
            Err(TryRecvError::Disconnected) => Err(io::Error::new(
                io::ErrorKind::NotConnected,
                format!("Lost the connection to {}", self.place),
            )),
        };
        match answer {
            Ok(Done::SignInPage {
                url,
                user_code,
                paste,
                ..
            }) => {
                cx.open_url(&url);
                self.flow = Flow::Browser {
                    page: Page {
                        url: url.into(),
                        user_code,
                        paste,
                    },
                    finishing: None,
                };
                if !paste {
                    self.finish(None, cx);
                }
            }
            Ok(Done::SignedIn { .. }) => {
                self.flow = Flow::SignedIn;
                cx.emit(SignInEvent::SignedIn);
            }
            Ok(_) => self.flow = Flow::Failed(format!("{} answered something else", self.place).into()),
            Err(error) => self.flow = Flow::Failed(said(&self.place, &error)),
        }
        cx.notify();
    }

    /// The sheet went: a sign-in still running on the Machine is stopped
    /// there, as a tell.
    pub fn gone(&mut self, cx: &mut App) {
        if matches!(self.flow, Flow::Starting(_) | Flow::Browser { .. }) {
            let _ = (self.ask)(SignInStep::Cancel, cx);
        }
    }

    fn title(&self) -> SharedString {
        format!("Sign in to {} on {}", self.provider, self.place).into()
    }

    /// A button that closes the sheet.
    fn closer(&self, id: &'static str, label: &'static str, cx: &mut Context<Self>) -> gpui::component::button::Button {
        ferrite_ui::components::ghost_button(id, label, cx)
            .debug_selector(move || id.to_string())
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                cx.emit(SignInEvent::Close);
            }))
    }

    /// The face of where the sign-in stands: its words, then what the
    /// footer's left and right hold.
    fn face(&self, window: &Window, cx: &mut Context<Self>) -> (Div, Div, Div) {
        let mut left = div();
        let mut right = div().flex().items_center().gap(px(SPACE_2));
        let body = div().flex().flex_col().gap(px(SPACE_3));
        let body = match &self.flow {
            Flow::Starting(_) => {
                right = right.child(self.closer("sign-in-cancel", "Cancel", cx));
                body.debug_selector(|| "sign-in-starting".into()).child(note(format!(
                    "Starting {}'s sign-in on {}\u{2026}",
                    self.provider.cli_name(),
                    self.place
                )))
            }
            Flow::Browser { page, finishing } => {
                let url = page.url.clone();
                left = left.child(
                    ferrite_ui::components::ghost_button("sign-in-open-page", "Open the page again", cx)
                        .debug_selector(|| "sign-in-open-page".into())
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                );
                right = right.child(self.closer("sign-in-cancel", "Cancel", cx));
                let address = ferrite_ui::field::shown_path(
                    SharedString::default(),
                    page.url.clone(),
                    ferrite_ui::components::copy_button("sign-in-copy-page", page.url.clone())
                        .tooltip_text("Copy address"),
                );
                match (&page.user_code, finishing) {
                    (Some(code), _) => body
                        .debug_selector(|| "sign-in-enter-code".into())
                        .child(note("Enter this code on the page in your browser"))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(SPACE_2))
                                .child(
                                    div()
                                        .debug_selector(|| "sign-in-user-code".into())
                                        .font_family(FONT_CODE)
                                        .text_size(px(FS_HOME_TITLE))
                                        .line_height(px(LH_HOME_TITLE))
                                        .text_color(rgb(INK_STRONG))
                                        .child(SharedString::from(code.reveal().to_string())),
                                )
                                .child(
                                    ferrite_ui::components::copy_button("sign-in-copy-code", code.reveal().to_string())
                                        .tooltip_text("Copy code"),
                                ),
                        )
                        .child(note(
                            "If the page refuses it, turn on device code sign-in in ChatGPT's security settings, then try again",
                        ))
                        .child(address),
                    (None, None) => {
                        let focused = self.code.read(cx).focus_handle(cx).is_focused(window);
                        let empty = self.code.read(cx).text().trim().is_empty();
                        right = right.child(
                            ferrite_ui::form::primary_button("sign-in-submit", "Sign in", empty, cx)
                                .debug_selector(|| "sign-in-submit".into())
                                .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                                    cx.stop_propagation();
                                    view.submit(cx);
                                })),
                        );
                        body.debug_selector(|| "sign-in-paste-code".into())
                            .child(note("Sign in in your browser, then paste the code it shows"))
                            .child(ferrite_ui::field::text_field(
                                "Code".into(),
                                self.code.clone(),
                                focused,
                                "sign-in-code-focused",
                            ))
                            .child(address)
                    }
                    (None, Some(_)) => body.debug_selector(|| "sign-in-finishing".into()).child(note("Signing in\u{2026}")),
                }
            }
            Flow::SignedIn => {
                right = right.child(
                    ferrite_ui::form::primary_button("sign-in-done", "Done", false, cx)
                        .debug_selector(|| "sign-in-done".into())
                        .on_click(cx.listener(|_, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            cx.emit(SignInEvent::Close);
                        })),
                );
                body.debug_selector(|| "sign-in-signed-in".into())
                    .child(note(format!("{} is signed in on {}", self.provider, self.place)))
            }
            Flow::Failed(why) => {
                right = right.child(self.closer("sign-in-close", "Close", cx)).child(
                    ferrite_ui::form::primary_button("sign-in-try-again", "Try again", false, cx)
                        .debug_selector(|| "sign-in-try-again".into())
                        .on_click(cx.listener(|view, _: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            view.start(cx);
                        })),
                );
                body.debug_selector(|| "sign-in-failed".into())
                    .child(ferrite_ui::form::error_line(why.clone()))
            }
        };
        (body, left, right)
    }

    /// The card, for the sheet host.
    pub fn card(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<ferrite_ui::sheet::Drawn> {
        let close = ferrite_ui::sheet::sheet_close("sign-in-x", "Close", cx).on_click(cx.listener(
            |_, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                cx.emit(SignInEvent::Close);
            },
        ));
        let (body, left, right) = self.face(window, cx);
        let card = ferrite_ui::sheet::sheet_fit(
            ferrite_ui::form::COMPACT_WIDTH,
            f32::from(window.viewport_size().height),
        )
        .id("sign-in-card")
        .debug_selector(|| "sign-in-card".into())
        .key_context(KEY_CONTEXT)
        .on_action(cx.listener(|view, _: &Submit, _, cx| view.submit(cx)))
        .track_focus(&self.focus)
        .occlude()
        .child(ferrite_ui::form::head(self.title(), close))
        .child(ferrite_ui::form::body().child(body))
        .child(ferrite_ui::form::footer(left, right));
        Some(ferrite_ui::sheet::Drawn { card, above: None })
    }
}

/// Ask the Machine `place` names to start the sign-in.
fn started(ask: &Ask, place: &str, cx: &mut App) -> Flow {
    match ask(SignInStep::Start, cx) {
        Ok(answer) => Flow::Starting(answer),
        Err(error) => Flow::Failed(said(place, &error)),
    }
}

/// What the Machine's failure reads as: a Server from before sign-ins is
/// told to update.
fn said(place: &str, error: &io::Error) -> SharedString {
    match error.kind() {
        io::ErrorKind::Unsupported => format!("Update Ferrite on {place} to sign in from here").into(),
        _ => error.to_string().into(),
    }
}

/// A line of the sheet's words.
fn note(text: impl Into<SharedString>) -> Div {
    ferrite_ui::components::text_ui()
        .text_color(rgb(INK_STRONG))
        .child(text.into())
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::sync::mpsc::{channel, Sender};

    use ferrite_ui::testing::{click, mount, record, shows, Heard};
    use gpui::{KeyBinding, TestAppContext, VisualTestContext};

    use super::*;

    /// The Machine's Server, as a test keeps it: each step asked, and the
    /// answers not given yet, oldest first. A refusing one answers no ask.
    #[derive(Default)]
    struct Machine {
        asked: Vec<SignInStep>,
        waiting: Vec<Sender<io::Result<Done>>>,
        refuses: Option<io::ErrorKind>,
    }

    impl Machine {
        fn answer(&mut self, answer: io::Result<Done>) {
            self.waiting.remove(0).send(answer).unwrap();
        }
    }

    /// The sheet's owner, as the sheet host draws it.
    struct Owner {
        view: Entity<SignInView>,
    }

    impl Render for Owner {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let card = self.view.update(cx, |view, cx| view.card(window, cx)).map(|drawn| drawn.card);
            div().size_full().children(card)
        }
    }

    /// The sheet for `provider` on `box` in a window of its own, asking
    /// `machine` as the Cockpit asks a Machine's Server.
    fn open(
        provider: Provider,
        machine: Machine,
        cx: &mut TestAppContext,
    ) -> (Entity<SignInView>, Rc<RefCell<Machine>>, Heard<SignInEvent>, &mut VisualTestContext) {
        cx.update(|cx| cx.bind_keys([KeyBinding::new("enter", Submit, Some(KEY_CONTEXT))]));
        let machine = Rc::new(RefCell::new(machine));
        let kept = machine.clone();
        let ask: Ask = Rc::new(move |step, _| {
            let mut machine = kept.borrow_mut();
            if let Some(kind) = machine.refuses {
                return Err(io::Error::new(kind, "refused"));
            }
            machine.asked.push(step);
            let (answer, answered) = channel();
            machine.waiting.push(answer);
            Ok(answered)
        });
        let (owner, cx) = mount(cx, move |_, cx| Owner {
            view: cx.new(|cx| SignInView::new(provider, "box".into(), ask, cx)),
        });
        let view = owner.read_with(cx, |owner, _| owner.view.clone());
        let heard = record(cx, &view, |event: &SignInEvent| *event);
        (view, machine, heard, cx)
    }

    /// The pump looks at the sheet, as the window's does.
    fn pump(view: &Entity<SignInView>, cx: &mut VisualTestContext) {
        view.update(cx, |view, cx| view.poll(cx));
        cx.run_until_parked();
    }

    fn says(cx: &mut VisualTestContext) -> String {
        cx.update(|window, _| window.debug_text("sign-in-card"))
            .unwrap_or_default()
            .join(" ")
    }

    fn claude_page() -> Done {
        Done::SignInPage {
            provider: Provider::Claude,
            url: "https://claude.ai/oauth/authorize?state=s3nt".into(),
            user_code: None,
            paste: true,
        }
    }

    #[gpui::test]
    fn claudes_sign_in_opens_its_page_and_takes_the_code_it_gave(cx: &mut TestAppContext) {
        let (view, machine, heard, cx) = open(Provider::Claude, Machine::default(), cx);
        assert_eq!(machine.borrow().asked, [SignInStep::Start]);
        assert!(shows(cx, "sign-in-starting"));
        assert!(says(cx).contains("Starting Claude Code's sign-in on box"), "{}", says(cx));

        machine.borrow_mut().answer(Ok(claude_page()));
        pump(&view, cx);
        assert_eq!(cx.opened_url().as_deref(), Some("https://claude.ai/oauth/authorize?state=s3nt"));
        assert!(shows(cx, "sign-in-paste-code"));
        let code = view.read_with(cx, |view, _| view.code.clone());
        code.update(cx, |code, cx| code.set("pasted-code#state".into(), cx));
        cx.update(|window, cx| window.focus(&code.read(cx).focus_handle(cx), cx));
        cx.simulate_keystrokes("enter");
        assert_eq!(
            machine.borrow().asked[1..],
            [SignInStep::Finish {
                code: Some(Withheld::new("pasted-code#state")),
            }]
        );
        assert!(shows(cx, "sign-in-finishing"));

        machine.borrow_mut().answer(Ok(Done::SignedIn { provider: Provider::Claude }));
        pump(&view, cx);
        assert!(shows(cx, "sign-in-signed-in"));
        assert!(says(cx).contains("Claude is signed in on box"), "{}", says(cx));
        assert_eq!(heard.take(), [SignInEvent::SignedIn]);
        click(cx, "sign-in-done");
        assert_eq!(heard.take(), [SignInEvent::Close]);
        view.update(cx, |view, cx| view.gone(cx));
        assert_eq!(machine.borrow().asked.len(), 2, "a sign-in that ended is not cancelled");
    }

    #[gpui::test]
    fn codex_shows_the_code_to_enter_and_waits_on_the_page(cx: &mut TestAppContext) {
        let (view, machine, _, cx) = open(Provider::Codex, Machine::default(), cx);
        machine.borrow_mut().answer(Ok(Done::SignInPage {
            provider: Provider::Codex,
            url: "https://auth.openai.com/codex/device".into(),
            user_code: Some(Withheld::new("WXYZ-12345")),
            paste: false,
        }));
        pump(&view, cx);
        assert_eq!(cx.opened_url().as_deref(), Some("https://auth.openai.com/codex/device"));
        assert!(shows(cx, "sign-in-user-code"));
        assert!(says(cx).contains("WXYZ-12345"), "{}", says(cx));
        assert_eq!(
            machine.borrow().asked,
            [SignInStep::Start, SignInStep::Finish { code: None }],
            "its CLI waits on the page's approval at once"
        );
    }

    #[gpui::test]
    fn try_again_starts_it_anew(cx: &mut TestAppContext) {
        let (view, machine, _, cx) = open(Provider::Claude, Machine::default(), cx);
        machine
            .borrow_mut()
            .answer(Err(io::Error::other("Claude Code did not say where to sign in")));
        pump(&view, cx);
        assert!(shows(cx, "sign-in-failed"));
        assert!(says(cx).contains("Claude Code did not say where to sign in"), "{}", says(cx));
        click(cx, "sign-in-try-again");
        assert_eq!(machine.borrow().asked, [SignInStep::Start, SignInStep::Start]);
        assert!(shows(cx, "sign-in-starting"));
    }

    #[gpui::test]
    fn closing_it_while_it_runs_cancels_it_there(cx: &mut TestAppContext) {
        let (view, machine, heard, cx) = open(Provider::Claude, Machine::default(), cx);
        machine.borrow_mut().answer(Ok(claude_page()));
        pump(&view, cx);
        click(cx, "sign-in-cancel");
        assert_eq!(heard.take(), [SignInEvent::Close]);
        view.update(cx, |view, cx| view.gone(cx));
        assert_eq!(machine.borrow().asked, [SignInStep::Start, SignInStep::Cancel]);
    }

    #[gpui::test]
    fn a_machine_from_before_sign_ins_is_told_to_update(cx: &mut TestAppContext) {
        let machine = Machine {
            refuses: Some(io::ErrorKind::Unsupported),
            ..Machine::default()
        };
        let (_, _, _, cx) = open(Provider::Codex, machine, cx);
        assert!(shows(cx, "sign-in-failed"));
        assert!(says(cx).contains("Update Ferrite on box to sign in from here"), "{}", says(cx));
    }
}
