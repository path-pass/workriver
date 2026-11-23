// The user's message to a manager: given to it, held, and what became of the delivery.
use super::*;

impl App {
    /// Whom a message to every manager goes to: the seat of each open workspace whose session
    /// runs now, here or on a followed machine. Refused when none does, or the runtime cannot say.
    pub(super) fn running_managers(&self) -> Result<Vec<core::Recipient>, ApiError> {
        let all = self.store.lock().unwrap().recorded()?;
        let live = self.runtime.agents().map_err(|_| ApiError(axum::http::StatusCode::SERVICE_UNAVAILABLE, words::NOT_CONNECTED.into()))?;
        let to = all.managers_among(&live);
        if to.is_empty() {
            return Err(bad(format!("no manager is running; {}", core::SEATING)));
        }
        Ok(to)
    }

    /// The user wrote to a manager (or to every manager: each copy alike): typed into its
    /// terminal now, held while it cannot take it, or recorded as not delivered (`hand`).
    pub(in crate::server) fn tell_manager(&self, id: &str) {
        // Without the message nothing is given to anyone: it stays as the page shows it, sent
        // and not delivered yet.
        let Ok(Some(all)) = got(self.store.lock().unwrap().recorded()) else { return };
        let Ok(m) = all.message(id) else { return };
        // To one manager: its message, `copy` empty; to every manager: each one's copy.
        let held: Vec<Held> = match m.to.is_empty() {
            true => vec![Held::new(&all, For::Message, id, &m.manager, self.user_message(&all, m, ""), None)],
            false => m.to.iter().map(|r| Held::new(&all, For::Broadcast, id, &r.manager, self.user_message(&all, m, &r.manager), None)).collect(),
        };
        drop(all);
        for h in held {
            self.hand(h);
        }
    }

    /// The user's message as the manager reads it: with files or lines picked (the one place
    /// that rule is), where the unit's branch is checked out, found here from git and given
    /// to `delivery`, which stays pure and names it whenever given. To every manager: `copy`'s.
    fn user_message(&self, all: &core::State, m: &core::Message, copy: &str) -> String {
        if let Some(r) = m.to.iter().find(|r| r.manager == copy) {
            return delivery::broadcast_message(all, m, r, &self.home);
        }
        let code = m.context.iter().any(|c| matches!(c.kind.as_str(), "file" | "lines"));
        let dir = all.unit(&m.unit).ok().filter(|_| code).map(|u| self.work_tree(u));
        delivery::user_message(all, m, &self.home, dir.as_deref())
    }

    /// Where a unit's branch is checked out, else its repository (a research unit: its
    /// project folder).
    pub(super) fn work_tree(&self, unit: &core::Unit) -> String {
        let at = self.runner(unit.machine.as_deref()).ok().filter(|_| !unit.branch.is_empty());
        at.and_then(|at| scan::worktree_of(&at, Path::new(&unit.repo), &unit.branch).ok().flatten()).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| unit.repo.clone())
    }

    /// Records where the user's message to a manager is (`copy`: to every manager, that
    /// manager's copy; else empty).
    pub(super) fn message_delivery(&self, id: &str, copy: &str, delivery: Delivery) {
        self.held.lock().unwrap().retain(|h| !(h.question == id && matches!(h.kind, For::Message | For::Broadcast) && (copy.is_empty() || h.to == copy)));
        if let Err(e) = self.record(&Principal::Service, Body::MessageDelivery { id: id.into(), manager: copy.into(), delivery }) {
            tracing::warn!("message {id}: {}", e.1);
        }
    }

    pub(in crate::server) fn message_not_delivered(&self, id: &str, copy: &str, to: Option<&str>, code: ReasonCode, why: &str) {
        // The delivery holds the message as it would have been given: without it, nothing is
        // recorded in its place.
        let Ok(Some(all)) = got(self.store.lock().unwrap().recorded()) else { return };
        let message = all.message(id).ok().map(|m| self.user_message(&all, m, copy)).unwrap_or_default();
        drop(all);
        self.message_delivery(id, copy, Delivery { step: DeliveryStep::NotDelivered, to: to.map(str::to_string), reason_code: Some(code), reason: Some(why.into()), message });
    }
}
