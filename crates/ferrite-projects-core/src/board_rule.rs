//! What an agent or a reviewer the board started is told about the board itself: able, not nudged. The board refuses
//! them no command, and their rules list no way to change the board: they leave it alone unless a human explicitly
//! told them to.
pub const TOLD: &str = "An explicit instruction in this task's own description, or in a message from the human, counts as being told.";
pub fn agent_board_rule() -> String { format!("Do nothing to the work board beyond your own task (reporting your own state, as said here) unless a human explicitly told you to. {TOLD} Work you merely come across is a line in your closing note or report, never a task.") }
pub fn reviewer_board_rule() -> String { format!("Do nothing to the work board unless a human explicitly told you to. {TOLD} A fault you merely come across goes in your findings, never onto the board: one that was there before this change is marked `preexisting`: it is a note for the human in your review, never sends the work back, and nothing else is done with it.") }
