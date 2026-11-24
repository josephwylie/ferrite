//! A window's `cli` command (`Op::Cli`), answered off the pump through
//! core's `Clis` seam.

use ferrite_core::protocol::CliAction;
use ferrite_engine::providers::clis::perform;

use super::{ClientId, Server};

impl Server {
    /// Answer a `cli` command on a thread of its own: a probe starts a
    /// process, a check reaches the network, an update runs an installer.
    pub(super) fn cli(&mut self, client: ClientId, request: u64, action: &CliAction) {
        let clis = std::sync::Arc::clone(&self.clis);
        let action = action.clone();
        self.read_off_pump(client, request, move || perform(&*clis, &action).map(Some));
    }
}
