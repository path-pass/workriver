use std::collections::{BTreeMap, BTreeSet};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::*;

/// Counts of recorded work on an intent, and elapsed time in its active intervals.
#[derive(Serialize, Deserialize, Debug)]
pub struct IntentTook {
    pub intent: String,
    pub kind: Option<IntentKind>,
    pub size: Option<IntentSize>,
    pub created: String,
    pub closed: Option<String>,
    pub active_days: f64,
    pub units: usize,
    pub units_by_kind: BTreeMap<String, usize>,
    pub tasks: usize,
    pub tasks_by_kind: BTreeMap<String, usize>,
    pub dropped_tasks: usize,
    pub review_rounds: BTreeMap<String, usize>,
}

impl State {
    pub fn intent_took(&self, intent: &Intent, now: SystemTime) -> Result<IntentTook, String> {
        let mut active_seconds = 0.0;
        let parse = |at: &str| humantime::parse_rfc3339_weak(at).map_err(|e| format!("{}: invalid event time {at:?}: {e}", intent.id));
        for (start, end) in &intent.active_intervals {
            let end = end.as_deref().map(parse).transpose()?.unwrap_or(now);
            active_seconds += end.duration_since(parse(start)?).map_err(|e| format!("{}: active interval ends before it starts: {e}", intent.id))?.as_secs_f64();
        }
        let mut took = IntentTook {
            intent: intent.id.clone(),
            kind: intent.kind,
            size: intent.size,
            created: intent.at.clone(),
            closed: intent.closed.clone(),
            active_days: active_seconds / 86_400.0,
            units: 0,
            units_by_kind: BTreeMap::new(),
            tasks: 0,
            tasks_by_kind: BTreeMap::new(),
            dropped_tasks: 0,
            review_rounds: BTreeMap::new(),
        };
        for unit in self.units.iter().filter(|u| u.intent == intent.id) {
            took.units += 1;
            *took.units_by_kind.entry(name(&unit.kind)).or_default() += 1;
            for task in unit.tasks.iter().filter(|task| !task.unfilled()) {
                took.tasks += 1;
                *took.tasks_by_kind.entry(name(&task.kind)).or_default() += 1;
                took.dropped_tasks += usize::from(task.state == TaskState::Dropped);
            }
            took.review_rounds.insert(unit.id.clone(), review_rounds(unit));
        }
        Ok(took)
    }
}

/// Each distinct implement or design task with a direct review is one round. Dropped
/// work and unfilled template steps do not start rounds; parallel reviewers share one.
fn review_rounds(unit: &Unit) -> usize {
    let in_round = |t: &&Task| t.state != TaskState::Dropped && !t.unfilled();
    let reviewed: BTreeSet<u32> = unit.tasks.iter().filter(in_round).filter(|t| t.kind == TaskKind::Review)
        .flat_map(|t| t.inputs.iter().filter_map(|i| i.from.parse().ok())).collect();
    unit.tasks.iter().filter(in_round).filter(|t| matches!(t.kind, TaskKind::Implement | TaskKind::Design) && reviewed.contains(&t.n)).count()
}
