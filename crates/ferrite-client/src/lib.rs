//! Ferrite's window model (ADR 0049): what a window holds of its
//! machines' Servers, and how it asks them. No UI.

pub mod cockpit;

#[cfg(test)]
#[path = "../../ferrite-core/src/test_temp.rs"]
mod test_temp;
