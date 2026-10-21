//! A part of the screen that fails to draw says so in its own place, in one red line, and every other part is drawn
//! as usual: the list, the text box and its hints never go with it. A fault there is caught here, quietly: the
//! terminal stays the board's (the panic hook leaves the terminal only for a fault nothing catches).
use std::cell::Cell;

thread_local! { static GUARDED: Cell<u32> = const { Cell::new(0) }; }
/// Is a part being drawn under a guard right now? The panic hook says nothing then.
pub fn quiet() -> bool { GUARDED.with(|g| g.get() > 0) }
/// Draw one part; Err with what went wrong when it failed.
pub fn part<T>(f: impl FnOnce() -> T) -> Result<T, String> {
    GUARDED.with(|g| g.set(g.get() + 1));
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    GUARDED.with(|g| g.set(g.get() - 1));
    r.map_err(|e| e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_else(|| "it failed".into()))
}
#[cfg(test)]
mod tests {
    #[test]
    fn a_failing_part_is_caught_and_said() {
        assert_eq!(super::part(|| 1), Ok(1));
        assert_eq!(super::part(|| -> i32 { panic!("nope") }), Err("nope".to_string()));
        assert!(!super::quiet());
    }
}
