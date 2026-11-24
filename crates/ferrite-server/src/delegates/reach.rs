//! What a caller asks of the linked machines themselves, answered off the
//! pump: `machine.list` with `reach`, T3 Code's linked environments list,
//! which asks every machine at once and gives each [`PROBE`] to answer
//! (`LIST_PROBE_TIMEOUT`); and `provider.list` or `project.list` of one
//! machine, what T3's `orchestrator_capabilities` and `t3_project_list`
//! with an `environmentId` list.

use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;

use ferrite_engine::api::schema::MachineListParams;
use ferrite_engine::api::{ApiError, CallerId, Code, Method};

use super::{refusal, words, PeerFailure, Peers, Server};

/// How long `machine.list` waits for each machine to answer.
const PROBE: Duration = Duration::from_secs(3);

/// A request whose answer is being worked out off the pump.
pub(super) struct Asked {
    caller: CallerId,
    id: String,
    answer: Receiver<Result<Value, ApiError>>,
}

impl Server {
    /// `machine.list`: the machines saved here; with `reach`, whether each
    /// answers now (`None`: the answer comes then).
    pub(crate) fn api_machine_list(
        &mut self,
        caller: CallerId,
        id: &str,
        params: &MachineListParams,
    ) -> Option<Result<Value, ApiError>> {
        let listed = self.machines();
        if !params.reach {
            return Some(listed);
        }
        let listed = match self.linked(None).and(listed) {
            Ok(listed) => listed,
            Err(refused) => return Some(Err(refused)),
        };
        let peers = Arc::clone(&self.delegation.peers);
        self.off_pump(caller, id, move || Ok(reached(&peers, listed)));
        None
    }

    /// `own` asked of the linked `machine` itself, as T3's tools with an
    /// `environmentId` are forwarded there: `provider.list` (what T3's
    /// `orchestrator_capabilities` lists) and `project.list` (its
    /// `t3_project_list`). The answer comes then.
    pub(crate) fn api_ask_machine(
        &mut self,
        caller: CallerId,
        id: &str,
        machine: String,
        own: Method,
    ) -> Option<Result<Value, ApiError>> {
        if let Err(refused) = self.linked(Some(&machine)) {
            return Some(Err(refused));
        }
        let peers = Arc::clone(&self.delegation.peers);
        self.off_pump(caller, id, move || {
            let failed = |failure: PeerFailure| refusal(&failure, &machine);
            let peer = peers.identify(&machine).map_err(failed)?;
            peers.call(&peer.server, own).map_err(failed)
        });
        None
    }

    /// Owe `caller` the answer to `id`, which `work` makes on a thread of
    /// its own.
    fn off_pump(
        &mut self,
        caller: CallerId,
        id: &str,
        work: impl FnOnce() -> Result<Value, ApiError> + Send + 'static,
    ) {
        let (tell, answer) = mpsc::channel();
        std::thread::spawn(move || {
            ferrite_core::spawn::serve_the_operator();
            let _ = tell.send(work());
        });
        self.api.owe(caller, id);
        self.delegation.asked.push(Asked {
            caller,
            id: id.to_string(),
            answer,
        });
    }

    /// Answer each request worked out since the last pass.
    pub(super) fn hear_asked(&mut self) {
        for asked in std::mem::take(&mut self.delegation.asked) {
            let answer = match asked.answer.try_recv() {
                Ok(answer) => answer,
                Err(mpsc::TryRecvError::Empty) => {
                    self.delegation.asked.push(asked);
                    continue;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    Err(ApiError::new(Code::Failed, "it ended with no answer"))
                }
            };
            self.api.reply(asked.caller, asked.id, answer);
        }
    }
}

/// `listed` (`machine.list`'s answer) with whether each machine answers
/// now: `status` `reachable`, `unreachable` or `sign_in`, and the
/// `problem` when not. All are asked at once.
fn reached(peers: &Arc<Peers>, mut listed: Value) -> Value {
    let Some(machines) = listed["machines"].as_array_mut() else {
        return listed;
    };
    let (tell, heard) = mpsc::channel();
    for (at, machine) in machines.iter().enumerate() {
        let name = machine["id"].as_str().unwrap_or_default().to_string();
        let (peers, tell) = (Arc::clone(peers), tell.clone());
        std::thread::spawn(move || {
            ferrite_core::spawn::serve_the_operator();
            let _ = tell.send((at, peers.identify(&name).map(|_| ())));
        });
    }
    drop(tell);
    let deadline = Instant::now() + PROBE;
    let mut answers = vec![None; machines.len()];
    while let Ok((at, answer)) = heard.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        answers[at] = Some(answer);
    }
    for (machine, answer) in machines.iter_mut().zip(answers) {
        let name = machine["id"].as_str().unwrap_or_default().to_string();
        let (status, problem) = match answer {
            Some(Ok(())) => ("reachable", None),
            Some(Err(failure @ PeerFailure::SignIn)) => ("sign_in", Some(words(&failure, &name))),
            Some(Err(failure @ PeerFailure::Expired)) => ("expired", Some(words(&failure, &name))),
            Some(Err(failure)) => ("unreachable", Some(words(&failure, &name))),
            None => (
                "unreachable",
                Some(format!("{name} did not answer within {} s.", PROBE.as_secs())),
            ),
        };
        machine["status"] = status.into();
        if let Some(problem) = problem {
            machine["problem"] = problem.into();
        }
    }
    listed
}
