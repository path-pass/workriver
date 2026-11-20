// The user's message to a manager: given to it, held, and what became of the delivery.
use super::*;

impl App {
    /// The user wrote to a manager: typed into its terminal now, or held while it waits for a
    /// person there (`hand`), or recorded as not delivered when it is not running.
    pub(in crate::server) fn tell_manager(&self, id: &str) {
        // Without the message nothing is given to anyone: it stays as the page shows it, sent
        // and not delivered yet.
        let Ok(Some(all)) = got(self.store.lock().unwrap().recorded()) else { return };
        let Some((to, text)) = all.message(id).ok().map(|m| (m.manager.clone(), delivery::user_message(&all, m))) else { return };
        // A manager with no name on record goes by its session, as everywhere.
        let name = all.name_of(&to).unwrap_or(&to).to_string();
        drop(all);
        match self.runtime.agents() {
            Err(_) => self.message_not_delivered(id, None, ReasonCode::RuntimeDown, words::NOT_CONNECTED),
            Ok(live) if !live.iter().any(|a| a.session == to) => self.message_not_delivered(id, None, ReasonCode::ManagerGone, &format!("the manager {name} is not running")),
            Ok(_) => self.hand(Held { question: id.into(), kind: For::Message, to, name, text, not_ready: None }),
        }
    }

    /// Records where the user's message to a manager is.
    pub(super) fn message_delivery(&self, id: &str, delivery: Delivery) {
        self.held.lock().unwrap().retain(|h| h.question != id);
        if let Err(e) = self.record(&Principal::Service, Body::MessageDelivery { id: id.into(), delivery }) {
            tracing::warn!("message {id}: {}", e.1);
        }
    }

    pub(in crate::server) fn message_not_delivered(&self, id: &str, to: Option<&str>, code: ReasonCode, why: &str) {
        // The delivery holds the message as it would have been given: without it, nothing is
        // recorded in its place.
        let Ok(Some(all)) = got(self.store.lock().unwrap().recorded()) else { return };
        let message = all.message(id).ok().map(|m| delivery::user_message(&all, m)).unwrap_or_default();
        drop(all);
        self.message_delivery(id, Delivery { step: DeliveryStep::NotDelivered, to: to.map(str::to_string), reason_code: Some(code), reason: Some(why.into()), message });
    }
}
