//! Ferrite's stand-in for upstream's `wake` (the terminal frame loop's wake, dropped): the
//! channel the search worker answers on. Ferrite's reader blocks on it, so a send wakes nothing.

pub type Sender<T> = std::sync::mpsc::Sender<T>;
