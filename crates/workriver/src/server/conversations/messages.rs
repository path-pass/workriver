// The user's message to a manager: given to it, held, and what became of the delivery.
use super::*;

impl App {
    /// The user wrote to a manager: typed into its terminal now, or held while it waits for a
    /// person there (`hand`), or recorded as not delivered when it is not running.
    pub(in crate::server) fn tell_manager(&self, id: &str) {
        let Some((to, text)) = ({
            let all = self.store.lock().unwrap().recorded();
            all.message(id).ok().map(|m| (m.manager.clone(), delivery::user_message(&all, m)))
        }) else {
            return;
        };
        let name = self.store.lock().unwrap().agent_name(&to).unwrap_or_else(|| to.clone());
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
        let message = {
            let all = self.store.lock().unwrap().recorded();
            all.message(id).ok().map(|m| delivery::user_message(&all, m)).unwrap_or_default()
        };
        self.message_delivery(id, Delivery { step: DeliveryStep::NotDelivered, to: to.map(str::to_string), reason_code: Some(code), reason: Some(why.into()), message });
    }
}
