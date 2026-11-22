// The graph of a unit's tasks: what each follows, what came before it, what follows from a
// finding, which inputs are unfinished, and the designs a task stands on (which gate asked
// about a design or a research result, which later design revised it, which one stands in
// its place).
use crate::*;

impl State {
    /// The tasks task `n` comes after through its inputs, however many between, oldest first.
    pub fn before(&self, owner: &str, n: u32) -> Vec<u32> {
        let mut seen = std::collections::BTreeSet::new();
        let inputs = |t: &Task| t.inputs.iter().filter_map(|i| i.from.parse::<u32>().ok()).collect::<Vec<_>>();
        let mut next = self.task(owner, n).map(inputs).unwrap_or_default();
        while let Some(k) = next.pop() {
            if let (true, Ok(b)) = (seen.insert(k), self.task(owner, k)) {
                next.extend(inputs(b));
            }
        }
        seen.into_iter().collect()
    }

    /// The designs a gate or a review is about: the design tasks it follows directly or
    /// through reviews only (design, its reviews, the gate). Not through implementing or
    /// another gate: a question to merge, asked after the work, is about no design.
    pub fn designs_asked(&self, owner: &str, n: u32) -> Vec<u32> {
        self.results_asked(owner, n, TaskKind::Design)
    }

    /// The tasks of `kind` (design or research) a gate or a review is about, as
    /// `designs_asked` finds designs.
    fn results_asked(&self, owner: &str, n: u32, kind: TaskKind) -> Vec<u32> {
        let mut found = std::collections::BTreeSet::new();
        self.before_through(owner, n, kind, &mut found);
        found.into_iter().collect()
    }

    fn before_through(&self, owner: &str, n: u32, kind: TaskKind, found: &mut std::collections::BTreeSet<u32>) {
        let Ok(t) = self.task(owner, n) else { return };
        for k in t.inputs.iter().filter_map(|i| i.from.parse::<u32>().ok()) {
            match self.task(owner, k).map(|b| b.kind) {
                Ok(x) if x == kind => {
                    found.insert(k);
                }
                Ok(TaskKind::Review) if k < n => self.before_through(owner, k, kind, found),
                _ => {}
            }
        }
    }

    /// The decide gate that asks the user to confirm design `d` (see `designs_asked`); the
    /// latest when it was asked again. None: nobody was asked.
    pub fn design_gate(&self, owner: &str, d: u32) -> Option<&Task> {
        let gates = self.tasks_of(owner)?.iter().filter(|g| g.gate == Some(GateKind::Decide) && g.dropped.is_none());
        gates.filter(|g| self.designs_asked(owner, g.n).contains(&d)).max_by_key(|g| g.n)
    }

    /// The gate that asks the user about design or research `n`: for a design its
    /// `design_gate`; for research the same walk through reviews, taking research. The latest
    /// not dropped. None: another kind, or nobody was asked (yet). A discussion of the task
    /// ends once it is submitted, and its agent is kept while it is not.
    pub fn result_gate(&self, owner: &str, n: u32) -> Option<&Task> {
        match self.task(owner, n).ok()?.kind {
            TaskKind::Design => self.design_gate(owner, n),
            TaskKind::Research => {
                let gates = self.tasks_of(owner)?.iter().filter(|g| g.gate == Some(GateKind::Decide) && g.dropped.is_none());
                gates.filter(|g| self.results_asked(owner, g.n, TaskKind::Research).contains(&n)).max_by_key(|g| g.n)
            }
            _ => None,
        }
    }

    /// The design tasks that revise design `d`, with their owner: those not dropped that
    /// answer a finding of the design itself or of a gate or review about it (see
    /// `designs_asked`). A design answering a finding about the code built from it does not.
    pub fn revised_by(&self, owner: &str, d: u32) -> Vec<(String, &Task)> {
        let hit = |at: &str, t: &Task| {
            t.kind == TaskKind::Design && t.dropped.is_none() && t.causes.iter().any(|c| c.unit.as_deref().unwrap_or(at) == owner && c.task != 0 && (c.task == d || self.designs_asked(owner, c.task).contains(&d)))
        };
        self.units.iter().flat_map(|u| u.tasks.iter().filter(|t| hit(&u.id, t)).map(|t| (u.id.clone(), t))).collect()
    }

    /// The design that stands in place of design `d`: its newest delivered revision, and
    /// that one's in turn. None while no revision of it is delivered: it stands itself.
    pub fn design_in_place(&self, owner: &str, d: u32) -> Option<(String, u32)> {
        let (mut at, mut seen, mut found) = ((owner.to_string(), d), vec![(owner.to_string(), d)], None);
        while let Some((o, t)) = self.revised_by(&at.0, at.1).into_iter().filter(|(_, t)| t.done.is_some()).max_by_key(|(_, t)| t.n) {
            at = (o, t.n);
            if seen.contains(&at) {
                break;
            }
            seen.push(at.clone());
            found = Some(at.clone());
        }
        found
    }

    /// The tasks handed out for a finding: those naming it as a cause, with their owner.
    pub fn follow_ups(&self, unit: &str, task: u32, finding: &str) -> Vec<(String, &Task)> {
        let hit = |owner: &str, t: &&Task| t.causes.iter().any(|c| c.task == task && c.unit.as_deref().unwrap_or(owner) == unit && c.findings.iter().any(|f| f == finding));
        self.units.iter().flat_map(|u| u.tasks.iter().filter(move |t| hit(&u.id, t)).map(move |t| (u.id.clone(), t))).collect()
    }

    /// The tasks a task follows that are not done yet (`start` is always done; a dropped
    /// task holds nothing up).
    pub fn unfinished_inputs(&self, owner: &str, t: &Task) -> Vec<String> {
        t.inputs
            .iter()
            .filter(|i| i.from != "start")
            .filter(|i| i.from.parse().ok().and_then(|n| self.task(owner, n).ok()).is_none_or(|x| !x.state.over()))
            .map(|i| format!("{owner}#{}", i.from))
            .collect()
    }
}
