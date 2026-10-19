//! Several attempts at one task, and one result kept: "best of three" with one agent, a "council" of different agents
//! with a judge. Both are the settings `attempts.by` (how many, and by whom) and `attempts.pick` (who chooses). Each
//! attempt is a small task of its own (store/attempts.rs) with its own agent, worktree, branch and tab in the task's one
//! herdr space. It goes up to the check and stops there. The check filters, the picker chooses; the chosen attempt then
//! becomes the task itself. Each area is a file in attempts/.
pub mod adopt;
pub mod compare;
pub mod diff;
pub mod fan_out;
pub mod place;
pub mod settle;
pub mod words;
