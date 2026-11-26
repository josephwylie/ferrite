//! The words and reads the Composer's footer draws with: whether a turn
//! runs, why its model cannot change now, the plugins its `+` menu lists,
//! and the effort a chip says.

use ferrite_client::cockpit::ThreadView;
use ferrite_core::provider::Provider;
use ferrite_ui::provider::effort_title;
use gpui::SharedString;

pub const TUNING_BUSY_HINT: &str = "Available when this turn finishes";
pub const TUNING_PENDING_HINT: &str = "Available once the last change is accepted";

/// Whether a turn runs on this Thread for the stop affordances: from the
/// moment a prompt is sent — before the provider's first event, which
/// `busy` waits for — through a starting Session to the turn's end. The
/// Composer reads it (`pane::primary_actions`), so a way to stop is in view
/// from the first frame.
pub fn turn_running(open: &ThreadView) -> bool {
    open.busy() || open.operator_turn()
}

/// Why a Thread's model and effort cannot change now, in the words the
/// pickers show; `None` while they can.
pub fn tuning_hold(open: &ThreadView) -> Option<&'static str> {
    if open.busy() {
        Some(TUNING_BUSY_HINT)
    } else if open.tuning() {
        Some(TUNING_PENDING_HINT)
    } else {
        None
    }
}

/// The plugins a Session's commands come from, each with its commands,
/// in their first command's order: what the provider says is a plugin's
/// (`CommandSource::Plugin`).
pub fn plugins(
    commands: &[ferrite_core::SessionCommand],
) -> Vec<(String, Vec<&ferrite_core::SessionCommand>)> {
    let mut plugins: Vec<(String, Vec<&ferrite_core::SessionCommand>)> = Vec::new();
    for command in commands {
        let Some(plugin) = command.source.as_ref().and_then(|source| source.plugin()) else {
            continue;
        };
        match plugins.iter_mut().find(|(name, _)| name == plugin) {
            Some((_, listed)) => listed.push(command),
            None => plugins.push((plugin.to_string(), vec![command])),
        }
    }
    plugins
}

/// An effort level as its Composer chip reads it: the title, lowercase
/// (`high`, `extra high`) — the chip is a quiet control, the menu rows keep
/// their titles.
pub fn effort_chip_label(effort: &str) -> SharedString {
    SharedString::from(effort_title(effort).to_lowercase())
}

/// The effort a chip shows (rule 2.6.5): the Thread's or draft's own
/// choice, else the operator's saved default, else the model row's own
/// default (`medium` for Codex). `None` when nothing resolves: a Thread's
/// chip then hides rather than reading the bare word `effort` (`/effort`
/// still reaches the picker), and a draft's band chip reads `default`.
fn effort_value(
    chosen: Option<&str>,
    saved: Option<&str>,
    provider: Provider,
    model: Option<&str>,
    announced: &[ferrite_core::ModelInfo],
) -> Option<SharedString> {
    if let Some(effort) = chosen.or(saved) {
        return Some(effort_chip_label(effort));
    }
    let rows = ferrite_core::models::catalog(provider, announced);
    let row = match model {
        Some(model) => rows.iter().find(|row| row.is(model)),
        None => rows.first(),
    };
    row.and_then(|row| row.default_effort.as_deref())
        .map(effort_chip_label)
}

/// The effort in force (FL-13): the chip's word (`effort_value`), else the
/// level the CLI itself runs at (`models::cli_default_effort`).
pub fn effort_in_force(
    chosen: Option<&str>,
    saved: Option<&str>,
    provider: Provider,
    model: Option<&str>,
    announced: &[ferrite_core::ModelInfo],
) -> Option<SharedString> {
    effort_value(chosen, saved, provider, model, announced).or_else(|| {
        ferrite_core::models::cli_default_effort(provider, model).map(effort_chip_label)
    })
}

/// The effort in force on an open Thread (`medium`): its own choice, else
/// the operator's default (`saved`), else its model's (`effort_in_force`);
/// `None` for a model that takes none.
pub fn thread_effort(open: &ThreadView, saved: Option<&str>) -> Option<SharedString> {
    let provider = open.provider();
    let ladder = ferrite_core::models::efforts_for(provider, open.model(), open.models());
    (!ladder.is_empty())
        .then(|| effort_in_force(open.effort(), saved, provider, open.model(), open.models()))
        .flatten()
}
