// The user's discussion of a delivered design or research with its own agent: each question
// given to that agent (held while it works, waits for a person or is on a blind task), never
// to a temporary one and with no deadline; failed when the agent is gone (from a read known
// to be complete) or when the gate that asks about the result is submitted; kept over a
// restart; and what the manager hears of it (an amendment while the gate waits, the
// conclusion), never each round.
use super::*;

impl App {
    /// A question of a discussion was recorded: given to its task's agent now, or held.
    pub(in crate::server) fn discuss(&self, id: &str) {
        let read = got(self.store.lock().unwrap().recorded());
        let all = match read {
            Ok(Some(all)) => all,
            Ok(None) => return,
            Err(Unread(why)) => return self.drop_question(id, ReasonCode::RuntimeError, &why),
        };
        let Ok(q) = all.question(id).cloned() else { return };
        let Some(session) = all.task(&q.unit, q.task).ok().and_then(|t| t.agent.clone()) else {
            return self.drop_question(id, ReasonCode::AgentGone, &format!("{}#{} has no agent", q.unit, q.task));
        };
        // Named as the runtime lists it, else as last recorded.
        let listed = self.live().unwrap_or_default().into_iter().find(|a| a.session == session).map(|a| a.name);
        let name = listed.or_else(|| all.name_of(&session).map(str::to_string)).unwrap_or_else(|| session.clone());
        let text = prompts::discussion(&all, &q, &self.home);
        drop(all);
        self.hand(Held { question: id.into(), kind: For::Discussion, to: session, name, text, not_ready: None, about: None });
    }

    /// A question of a discussion was typed into its agent: recorded as given to it, an
    /// answerer workriver never closes.
    pub(in crate::server) fn discussion_given(&self, h: &Held) {
        let body = Body::QuestionStarted { question: h.question.clone(), agent: h.to.clone(), name: h.name.clone(), temporary: false };
        if let Err(e) = self.record(&Principal::Service, body) {
            tracing::warn!("question {}: {}", h.question, e.1);
        }
    }

    /// A gate was submitted: the open questions of the discussions it ends fail, so no answer
    /// or amendment of a discussion comes after what the user confirmed.
    pub(in crate::server) fn gate_submitted(&self, unit: &str, gate: u32) {
        let Ok(Some(all)) = got(self.store.lock().unwrap().recorded()) else { return };
        let over: Vec<(String, String)> = all
            .questions
            .iter()
            .filter(|q| q.discuss && q.open() && q.unit == unit && all.result_gate(&q.unit, q.task).is_some_and(|g| g.n == gate))
            .map(|q| (q.id.clone(), format!("its gate {unit}#{gate} was submitted: the discussion of {}#{} is over", q.unit, q.task)))
            .collect();
        drop(all);
        for (id, why) in over {
            self.drop_question(&id, ReasonCode::GateSubmitted, &why);
        }
    }

    /// Open discussion questions whose agent is gone fail, held or given. Gone only from a
    /// list known to be complete for its machine (`readable`, as the idle reminder): a failed
    /// or empty read, or a machine not reached, says nothing.
    pub(in crate::server) fn discussions_gone(&self, live: &[RuntimeAgent], read: &[Option<String>]) {
        let Ok(Some(open)) = got(self.store.lock().unwrap().open_discussions()) else { return };
        for d in open {
            let Some(a) = d.agent else { continue };
            if !read.contains(&d.machine) || live.iter().any(|x| x.session == a || x.name == a) {
                continue;
            }
            // By the name it was given the question under, else as last recorded.
            let name = d.question.name.clone().or_else(|| got(self.store.lock().unwrap().agent_name(&a)).ok().flatten().flatten()).unwrap_or(a);
            self.drop_question(&d.question.id, ReasonCode::AgentGone, &format!("{} is no longer running", delivery::quote::name(&name)));
        }
    }

    /// At start: an open discussion question is still its agent's (nothing was lost); one a
    /// previous run never gave is given now.
    pub(in crate::server) fn discussions_again(&self) {
        let open = got(self.store.lock().unwrap().open_discussions()).ok().flatten().unwrap_or_default();
        for d in open.into_iter().filter(|d| d.question.agent.is_none()) {
            self.discuss(&d.question.id);
        }
    }

    /// The manager hears of a discussion what it must act on: the result amended while its
    /// gate is not submitted, and the conclusion. The rest it reads in `task show`.
    pub(in crate::server) fn tell_of_discussion(&self, e: &core::Event) {
        let live = self.live();
        let Ok(Some(all)) = got(self.store.lock().unwrap().recorded()) else { return };
        let to = delivery::discussed(&all, live.as_deref(), e);
        drop(all);
        for (agent, message) in to {
            if let Err(err) = self.send(&agent, &message) {
                tracing::warn!("cannot tell {agent} of a discussion: {err}");
            }
        }
    }
}
