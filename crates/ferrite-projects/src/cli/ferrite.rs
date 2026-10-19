//! Ferrite: what the command line answers differently when Ferrite ships it (ADR 0031). Ferrite's app carries this
//! program as `ferrite-projects` and updates it with itself, and its Server runs `take-over` at its start.

use crate::store::files::user_home;
use crate::store::move_home::{env_now, move_note, move_told};

/// `install` and `update`: nothing to do here, Ferrite does both.
pub const SHIPPED: &str = "ferrite-projects ships inside Ferrite and is updated with it: update Ferrite to update it. \
                           Nothing was changed.";

/// `take-over`: Ferrite's Server runs it at its start, before the keeper. Like any command, it has taken over
/// older installs first (`take_over::at_start`, then the moves after it); this says what they did, once, for the
/// Server to show. Empty when there was nothing to say, or it was said before.
pub fn take_over_said() -> String {
    let (home, env) = (user_home(), env_now());
    let said = move_note(&home, &env);
    move_told(&home, &env);
    said
}

/// What these commands say: the text, a line of its own; with `--json`, `{"said": …}` (empty when nothing was said).
pub fn say(said: &str, json: bool) {
    if json { println!("{}", serde_json::json!({ "said": said })); } else if !said.is_empty() { println!("{said}"); }
}
