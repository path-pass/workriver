// The user's discussion of a delivered design or research with its own agent: each question
// given to that agent (held while it works, waits for a person or is on a blind task), never
// to a temporary one and with no deadline; failed when the agent is gone (from a read known
// to be complete) or when the gate that asks about the result is submitted; kept over a
// restart; and what the manager hears of it (an amendment while the gate waits, the
// conclusion), never each round.
use std::collections::{HashMap, HashSet};

use super::*;

/// The sessions gone, as of this read: each listed session's machine is remembered in `seen`
/// (none: this one); a session is gone when its machine's read now is complete (`read`, as
/// `App::readable` gives it, the idle reminder's rule) and does not list it. A session never
/// listed, or whose machine was not read now, is unknown: not gone. The author of a result
/// on a followed machine is judged on that machine, not on its unit's.
pub(in crate::server) fn gone_from(seen: &mut HashMap<String, Option<String>>, live: &[RuntimeAgent], read: &[Option<String>]) -> HashSet<String> {
    for a in live {
        seen.insert(a.session.clone(), a.machine.clone());
    }
    seen.iter().filter(|(s, m)| read.contains(m) && !live.iter().any(|a| &a.session == *s)).map(|(s, _)| s.clone()).collect()
}

/// Why a discussion's question is over (its result's gate was submitted), if it is.
pub(super) fn over(state: &core::State, q: &core::Question) -> Option<String> {
    let g = state.result_gate(&q.unit, q.task).filter(|g| g.state == core::TaskState::Done)?;
    Some(format!("its gate {}#{} was submitted: the discussion of {}#{} is over", q.unit, g.n, q.unit, q.task))
}

impl App {
    /// A question of a discussion was recorded (or a previous run left it never given): given
    /// to its task's agent now, or held; failed if the result's gate was submitted meanwhile.
    pub(in crate::server) fn discuss(&self, id: &str) {
        let read = got(self.store.lock().unwrap().recorded());
        let all = match read {
            Ok(Some(all)) => all,
            Ok(None) => return,
            Err(Unread(why)) => return self.drop_question(id, ReasonCode::RuntimeError, &why),
        };
        let Ok(q) = all.question(id).cloned() else { return };
        if let Some(why) = over(&all, &q) {
            drop(all);
            return self.drop_question(id, ReasonCode::GateSubmitted, &why);
        }
        let Some(session) = all.task(&q.unit, q.task).ok().and_then(|t| t.agent.clone()) else {
            return self.drop_question(id, ReasonCode::AgentGone, &format!("{}#{} has no agent", q.unit, q.task));
        };
        let text = prompts::discussion(&all, &q, &self.home);
        let h = Held::new(&all, For::Discussion, id, &session, text, None);
        drop(all);
        self.hand(h);
    }

    /// A question of a discussion was typed into its agent: recorded as given to it, an
    /// answerer workriver never closes.
    pub(in crate::server) fn discussion_given(&self, h: &Held) {
        let body = Body::QuestionStarted { question: h.question.clone(), agent: h.to.clone(), name: h.name.clone(), temporary: false };
        if let Err(e) = self.record(&Principal::Service, body) {
            tracing::warn!("question {}: {}", h.question, e.1);
        }
    }

    /// Open discussion questions whose agent is gone (`gone_from`) fail, held or given.
    pub(in crate::server) fn discussions_gone(&self, gone: &HashSet<String>) {
        let Ok(Some(open)) = got(self.store.lock().unwrap().open_discussions()) else { return };
        for d in open {
            let Some(a) = d.agent.filter(|a| gone.contains(a)) else { continue };
            // By the name it was given the question under, else as last recorded.
            let name = d.question.name.clone().or_else(|| got(self.store.lock().unwrap().agent_name(&a)).ok().flatten().flatten()).unwrap_or(a);
            self.drop_question(&d.question.id, ReasonCode::AgentGone, &format!("{} is no longer running", delivery::quote::name(&name)));
        }
    }

    /// At start: an open discussion question is still its agent's (nothing was lost); one a
    /// previous run never gave is given now. But first, given or not, it fails if its gate was
    /// submitted (`discussions_over`).
    pub(in crate::server) fn discussions_again(&self) {
        let left = self.discussions_over();
        for id in left {
            self.discuss(&id);
        }
    }

    /// Open discussion questions whose result's gate was submitted fail, given or held: a run
    /// that stopped, or a read that failed, between recording the gate and failing them left
    /// them open. At start and at every poll. The open ones never given yet, the rest.
    pub(in crate::server) fn discussions_over(&self) -> Vec<String> {
        let Ok(Some(all)) = got(self.store.lock().unwrap().recorded()) else { return vec![] };
        let open: Vec<(String, bool, Option<String>)> = all.questions.iter().filter(|q| q.discuss && q.open()).map(|q| (q.id.clone(), q.agent.is_some(), over(&all, q))).collect();
        drop(all);
        let mut left = vec![];
        for (id, given, ended) in open {
            match ended {
                Some(why) => self.drop_question(&id, ReasonCode::GateSubmitted, &why),
                None if !given => left.push(id),
                None => {}
            }
        }
        left
    }

    /// The manager hears of a discussion what it must act on: the result amended while its
    /// gate is not submitted, and the conclusion. The rest it reads in `task show`.
    /// Given through `hand`: held until the manager can take it.
    pub(in crate::server) fn tell_of_discussion(&self, e: &core::Event) {
        let live = self.live();
        let Ok(Some(all)) = got(self.store.lock().unwrap().recorded()) else { return };
        let held: Vec<Held> = delivery::discussed(&all, live.as_deref(), e).into_iter().map(|(agent, text)| Held::notice(&all, &agent, text, None)).collect();
        drop(all);
        for h in held {
            self.hand(h);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use workriver_core::runtime::{AgentState, RuntimeAgent};

    use super::gone_from;

    fn agent(session: &str, machine: Option<&str>) -> RuntimeAgent {
        RuntimeAgent { session: session.into(), harness: "claude".into(), state: AgentState::Idle, cwd: String::new(), group: None, name: session.into(), topic: None, machine: machine.map(str::to_string) }
    }

    /// An agent is gone only when it was listed on a machine before and that machine's read
    /// now is complete without it; never seen, or its machine not read now: unknown, not gone.
    #[test]
    fn gone_is_judged_on_the_authors_own_machine() {
        let boss = agent("boss", None);
        let read_here = [None];
        let read_both = [None, Some("box".to_string())];
        // A researcher on a followed machine, never listed yet: the local read says nothing of it.
        let mut seen = HashMap::new();
        let gone = gone_from(&mut seen, std::slice::from_ref(&boss), &read_here);
        assert!(!gone.contains("r"));
        // Listed on box, then box not read: unknown.
        let gone = gone_from(&mut seen, &[boss.clone(), agent("r", Some("box"))], &read_both);
        assert!(gone.is_empty());
        let gone = gone_from(&mut seen, std::slice::from_ref(&boss), &read_here);
        assert!(!gone.contains("r"), "box was not read");
        // Box read and r not there: gone. Listed again: not gone.
        let gone = gone_from(&mut seen, std::slice::from_ref(&boss), &read_both);
        assert!(gone.contains("r"));
        let gone = gone_from(&mut seen, &[boss, agent("r", Some("box"))], &read_both);
        assert!(!gone.contains("r"));
    }
}
