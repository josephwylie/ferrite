//! A wait in a circle, across machines too: a way from a task, through what each task waits for, back to it. The
//! tasks of this machine are read here; a task of another machine is followed by asking that machine what it waits
//! for (after/far_ask.rs), and a task there that waits for one of this machine's comes back here.
use super::far::{id_of, is_far, split};
use super::far_ask::ask;
use crate::json::*;
use crate::remote::{own_names::own_names, short_label, why::known_host, Machine};
use crate::store::tasks::{list, ListOpts};
use std::collections::{HashMap, HashSet};

/// The most answers of other machines one check follows: a wait that goes on further is refused, not taken unchecked.
const MAX_ASKS: usize = 64;

/// Whether a machine label, as another machine names one of its own, is this machine.
pub fn is_me(label: &str) -> bool {
    own_names().iter().any(|n| n.eq_ignore_ascii_case(label) || (n.parse::<std::net::IpAddr>().is_err() && short_label(n).eq_ignore_ascii_case(label)))
}
/// The host a machine's target names: `vm-str` of `me@vm-str` or `ssh://me@vm-str:2200`.
fn host_of(target: &str) -> String {
    let k = known_host(target);
    match k.strip_prefix('[') { Some(r) => r.split(']').next().unwrap_or("").to_string(), None => k }
}
/// What tasks wait for, as far as it is known, and what each task of another machine is called there.
pub struct Graph { edges: HashMap<String, Vec<String>>, names: HashMap<String, String>, asked: HashSet<String>, machines: Vec<Machine> }
impl Graph {
    /// This machine's tasks, with `me` waiting for `deps`.
    pub fn new(me: &str, deps: &[String], machines: Vec<Machine>) -> Graph {
        let mut edges: HashMap<String, Vec<String>> = list(ListOpts { include_archived: true, ..Default::default() }).iter().map(|t| (t.t("id"), super::after_of(t))).collect();
        edges.insert(me.to_string(), deps.to_vec());
        Graph { edges, names: HashMap::new(), asked: HashSet::new(), machines }
    }
    /// A task as machine `label` has it in what it waits for, as this machine names it. `hosts`: where that machine
    /// reaches the machines it names, by which this machine knows itself, or one of its own, among them.
    fn ours(&self, label: &str, raw: &str, hosts: &Value) -> String {
        let Some((p, rest)) = split(raw) else { return id_of(label, raw) };
        let target = hosts.t(&p);
        if is_me(&p) || (!target.is_empty() && is_me(&host_of(&target))) { return rest; }
        let mine = self.machines.iter().find(|m| (!target.is_empty() && host_of(&m.target).eq_ignore_ascii_case(&host_of(&target))) || m.label.eq_ignore_ascii_case(&p));
        match mine { Some(m) => id_of(&m.label, &rest), None => format!("?{p}:{rest}") }
    }
    /// One answer of machine `label` (after/far_ask.rs) taken in: what its tasks wait for, and their names.
    pub fn take_in(&mut self, label: &str, a: &Value) {
        for (x, raws) in a.at("edges").as_object().cloned().unwrap_or_default() {
            let node = id_of(label, &x);
            let next: Vec<String> = strings(&raws).iter().map(|r| self.ours(label, r, a.at("machines"))).collect();
            self.edges.insert(node.clone(), next);
            let name = a.at("names").s(&x);
            self.names.insert(node.clone(), id_of(label, if name.is_empty() { &x } else { name }));
            self.asked.insert(node);
        }
    }
    /// The tasks of other machines that `me` reaches and nobody asked about yet (`?vmx:…`: of a machine another one
    /// named and this board does not know).
    fn unasked(&self, me: &str) -> Vec<String> {
        let (mut seen, mut todo, mut out) = (HashSet::new(), vec![me.to_string()], vec![]);
        while let Some(n) = todo.pop() {
            if !seen.insert(n.clone()) { continue; }
            if is_far(&n) && !self.asked.contains(&n) { out.push(n.clone()); }
            todo.extend(self.edges.get(&n).cloned().unwrap_or_default());
        }
        out
    }
    /// A way from `me` back to `me`, asking the machines on the way. Err: the way could not be followed to its end (a
    /// machine on it did not answer, or is not one the board knows), so whether there is a circle is not known.
    pub fn circle(&mut self, me: &str) -> Result<Option<Vec<String>>, String> {
        let mut stuck = None;
        for round in 0.. {
            let open = self.unasked(me);
            let Some(first) = open.first() else { break };
            if let Some(u) = open.iter().find(|i| i.starts_with('?')) { stuck = Some(format!("a task on the way waits for one of {}, a machine this board does not know", u[1..].split(':').next().unwrap_or(""))); break; }
            if round == MAX_ASKS { stuck = Some(format!("the tasks it would wait for go on through more than {MAX_ASKS} answers of other machines")); break; }
            if self.machines.is_empty() { self.machines = crate::remote::machines(); } // reached through a task here that waits on one
            let label = first.split(':').next().unwrap_or("").to_string();
            let ids: Vec<String> = open.iter().filter(|i| i.starts_with(&format!("{label}:"))).cloned().collect();
            let locals: Vec<String> = ids.iter().map(|i| i[label.len() + 1..].to_string()).collect();
            let answers = match self.machines.iter().find(|m| m.label == label).cloned() {
                Some(m) => ask(&m, &locals).map_err(|why| format!("{label} did not answer ({why})")),
                None => Err(format!("{label} is not a machine the board knows any more")),
            };
            match answers {
                Ok(list) => { for a in list.iter().filter(|a| a.b("found")) { self.take_in(&label, a); } }
                Err(w) => { stuck = Some(w); break; }
            }
            self.asked.extend(ids);
        }
        fn walk(at: &str, me: &str, edges: &HashMap<String, Vec<String>>, seen: &mut Vec<String>, path: &mut Vec<String>) -> bool {
            for next in edges.get(at).cloned().unwrap_or_default() {
                path.push(next.clone());
                if next == me { return true; }
                if !seen.contains(&next) { seen.push(next.clone()); if walk(&next, me, edges, seen, path) { return true; } }
                path.pop();
            }
            false
        }
        let mut path = vec![me.to_string()];
        // A circle already found is said as one, however far the rest could be followed.
        if walk(me, me, &self.edges, &mut vec![], &mut path) { return Ok(Some(path)); }
        match stuck { Some(w) => Err(w), None => Ok(None) }
    }
    /// A task on the way, as a person names it.
    pub fn say(&self, id: &str) -> String { self.names.get(id).cloned().unwrap_or_else(|| super::say(id)) }
}
