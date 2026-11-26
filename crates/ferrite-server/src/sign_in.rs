//! A window's `sign_in` (`Op::SignIn`, ADR 0051): this machine's CLI signed
//! in by its own sign-in, a step at a time. Each sign-in running is kept
//! with the client that started it: only that client finishes or cancels
//! it, and it ends with that client, or with the Server.

use std::sync::Arc;

use ferrite_core::protocol::{Done, ErrorCode, SignInStep, WireError};
use ferrite_core::provider::Provider;
use ferrite_engine::connection::Heard;
use ferrite_engine::providers::sign_in::{self, SignIn};

use super::{failed, ClientId, Server};

/// A sign-in running on this machine. Dropped, it stops its CLI.
pub(super) struct Running {
    pub(super) provider: Provider,
    pub(super) client: ClientId,
    sign_in: SignIn,
}

impl Server {
    pub(super) fn sign_in(&mut self, client: ClientId, request: u64, provider: Provider, step: &SignInStep) {
        let heard = self.heard.clone();
        let answer = move |answer| {
            let _ = heard.send(Heard::Answered(client, request, answer));
        };
        match step {
            SignInStep::Status => {
                let clis = Arc::clone(&self.clis);
                self.read_off_pump(client, request, move || {
                    let signed_in = clis
                        .binary(provider)
                        .and_then(|binary| sign_in::status(provider, &binary.path));
                    Ok(Some(Done::SignInStatus { provider, signed_in }))
                });
            }
            SignInStep::Start => {
                self.sign_ins.retain(|running| running.provider != provider);
                let sign_in = SignIn::start(provider, Arc::clone(&self.clis), move |page| {
                    answer(
                        page.map(|page| {
                            Some(Done::SignInPage {
                                provider,
                                url: page.url,
                                user_code: page.user_code,
                                paste: page.paste,
                            })
                        })
                        .map_err(failed),
                    )
                });
                self.sign_ins.push(Running {
                    provider,
                    client,
                    sign_in,
                });
            }
            SignInStep::Finish { code } => {
                let running = self
                    .sign_ins
                    .iter()
                    .find(|running| running.provider == provider && running.client == client);
                match running {
                    Some(running) => running.sign_in.finish(code.clone(), move |finished| {
                        answer(finished.map(|()| Some(Done::SignedIn { provider })).map_err(failed))
                    }),
                    None => self.reply(
                        client,
                        request,
                        Err(failed(format!("No sign-in to {provider} is running there"))),
                    ),
                }
            }
            SignInStep::Cancel => {
                let before = self.sign_ins.len();
                self.sign_ins
                    .retain(|running| running.provider != provider || running.client != client);
                let changed = self.sign_ins.len() < before;
                self.reply(client, request, Ok(Some(Done::Changed { changed })));
            }
            SignInStep::Unknown => self.reply(
                client,
                request,
                Err(WireError {
                    code: ErrorCode::Unsupported,
                    message: "this server does not know that sign-in step".into(),
                }),
            ),
        }
    }

    /// A reply made off the pump. A sign-in that ended signed in first has
    /// its Provider's Sessions restart as they settle and its models asked
    /// for again, as `cli_replaced` has them.
    pub(super) fn answered(&mut self, client: ClientId, request: u64, answer: Result<Option<Done>, WireError>) {
        if let Ok(Some(Done::SignedIn { provider })) = &answer {
            self.engine.end_stale_sessions(*provider);
            self.engine.rediscover_models();
        }
        self.reply(client, request, answer);
    }
}
