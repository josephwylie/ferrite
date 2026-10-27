//! Binding the key table (ADR 0024): the defaults with the operator's keys
//! laid over them, in place of the table bound before.

use gpui::{App, DummyKeyboardMapper, KeyBinding, KeyBindingContextPredicate, KeyBindingMetaIndex};

use ferrite_ui::keymap::{self, Binding};

/// Marks the bindings Ferrite's key table made, apart from the ones the
/// kit (its inputs, text views, dialogs) and `rich` bind for themselves.
pub const KEY_TABLE: KeyBindingMetaIndex = KeyBindingMetaIndex(0);

/// Bind `table` in place of the key table bound before, and make it the
/// table every drawn chord reads.
///
/// gpui removes no single binding, so the keymap is rebuilt: everyone
/// else's bindings stay, in order, and the new table takes the old one's
/// place (the end, the first time), keeping same-depth precedence.
pub fn bind(table: Vec<Binding>, cx: &mut App) {
    keymap::install(table.clone());
    let mut table = Some(load(table, cx));
    let keymap = cx.key_bindings();
    let mut rebuilt = Vec::new();
    for binding in keymap.borrow().bindings() {
        if binding.meta() == Some(KEY_TABLE) {
            rebuilt.extend(table.take().into_iter().flatten());
        } else {
            rebuilt.push(binding.clone());
        }
    }
    rebuilt.extend(table.into_iter().flatten());
    cx.clear_key_bindings();
    cx.bind_keys(rebuilt);
}

/// A key table, built into live bindings. The table is data so both
/// platforms' spellings stay testable; actions are rebuilt from their
/// registered names here, and the app's tests keep a renamed action from
/// surviving to a launch panic.
pub fn load(table: Vec<Binding>, cx: &mut App) -> Vec<KeyBinding> {
    table
        .into_iter()
        .map(|(keystroke, action, context)| {
            let action = cx
                .build_action(action, None)
                .expect("the keymap names registered actions");
            let context = context.map(|context| {
                KeyBindingContextPredicate::parse(context)
                    .expect("the keymap contexts parse")
                    .into()
            });
            KeyBinding::load(
                &keystroke,
                action,
                context,
                false,
                None,
                &DummyKeyboardMapper,
            )
            .expect("the keymap keystrokes parse")
            .with_meta(KEY_TABLE)
        })
        .collect()
}
