// What the user answered from the page in an agent's terminal: the keys, what came of them,
// and the two pieces of the screen the record keeps (cut here, whatever the request says).
use serde::{Deserialize, Serialize};

use crate::screens::{KEPT_WORDS, Key, KeptOption, Outcome, kept, question_line};
use crate::*;

/// The most keys one answer sends: a step per option of a list of nine, and Enter.
pub const MOST_KEYS: usize = 10;

/// One answer, on the task its agent held.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct TerminalAnswer {
    pub at: String,
    pub keys: Vec<Key>,
    pub outcome: Outcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub option: Option<KeptOption>,
}

impl State {
    /// The rules of an answer (`prepare`): the user's alone (checked for every user-only
    /// kind); keys pressed; a task, when named, that is there; and of the screen only a
    /// question line that asks something and the option's number and words, cut.
    pub(crate) fn prepare_answers(&self, _who: &Principal, body: Body) -> Result<Body, Rejected> {
        let Body::AgentKeys { agent, machine, unit, task, keys, outcome, question, option } = body else {
            unreachable!("{} is not an answer", name_of_kind(&body))
        };
        if blank(&agent) {
            return no("an answer names the agent it was given to");
        }
        if keys.is_empty() || keys.len() > MOST_KEYS {
            return no(format!("an answer sends from 1 to {MOST_KEYS} keys"));
        }
        if let (Some(u), Some(n)) = (&unit, task) {
            self.task(u, n)?;
        } else if task.is_some() {
            return no("a task is named with its unit");
        }
        let question = question_line(question.as_deref());
        let option = option.map(|o| KeptOption { number: o.number.filter(|n| (1..=9).contains(n)), words: kept(&o.words, KEPT_WORDS) });
        Ok(Body::AgentKeys { agent, machine, unit, task, keys, outcome, question, option })
    }

    /// What an answer changes (`apply`): the task its agent held shows it.
    pub(crate) fn apply_answers(&mut self, e: &Event) {
        let Body::AgentKeys { unit: Some(u), task: Some(n), keys, outcome, question, option, .. } = &e.body else { return };
        if let Some(t) = self.task_mut(u, *n) {
            t.answers.push(TerminalAnswer { at: e.at.clone(), keys: keys.clone(), outcome: *outcome, question: question.clone(), option: option.clone() });
        }
    }
}
