// The terminal view: one agent's terminal as Server-Sent Events, one event per frame, from one
// runtime stream (today a child process) per open view, not shared, since each viewer's
// first frame must be whole. Capped across every page; each stream's queue is bounded; a
// stream that does not start with a whole frame, or has a frame larger than a bound, ends
// there, as does one whose reader falls behind (the page opens it again from a whole frame):
// a change is never passed on without the frames before it.
use std::convert::Infallible;
use std::sync::atomic::Ordering;

use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;

use super::*;
use crate::common::config::defaults::{VIEW_FRAME_MAX, VIEW_QUEUE, VIEWS_MAX};

/// One of the open views' slots, given back when its stream ends.
struct Slot(Arc<AtomicUsize>);

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// `GET /api/agent/view`: the terminal of the one place that shows the session (a seated
/// manager's: where its seat is held to, as the last tick found it), as it is drawn, from a
/// whole frame on; it ends when the place goes.
pub(in crate::server) async fn agent_view(State(app): State<Shared>, Extension(caller): Extension<Caller>, headers: HeaderMap, Query(q): Query<Which>) -> Result<Response, Refusal> {
    app.user(&caller)?;
    from_our_page(&caller, &headers)?;
    let views = app.terminals.views.clone();
    if views.try_update(Ordering::SeqCst, Ordering::SeqCst, |n| (n < VIEWS_MAX).then_some(n + 1)).is_err() {
        return Err(Refusal::new(StatusCode::SERVICE_UNAVAILABLE, "views", format!("{VIEWS_MAX} terminal views are open already; close one")));
    }
    let slot = Slot(views);
    let at = app.clone();
    let frames = tokio::task::spawn_blocking(move || -> Result<_, Refusal> {
        let machine = q.machine();
        let place = at.seat_place(&q.agent, machine).unwrap_or_else(|| at.runtime.resolve(machine, &q.agent))?;
        Ok(at.runtime.observe(machine, &place)?)
    })
    .await
    .map_err(|e| Refusal::new(StatusCode::INTERNAL_SERVER_ERROR, "failed", e.to_string()))??;
    let (tx, rx) = tokio::sync::mpsc::channel::<SseEvent>(VIEW_QUEUE);
    let stopping = app.stopping.subscribe();
    std::thread::spawn(move || {
        let _slot = slot;
        let event = |kind: &str, data: Value| SseEvent::default().event(kind).data(data.to_string());
        let mut whole = false;
        for item in frames {
            if *stopping.borrow() || tx.is_closed() {
                break;
            }
            let sent = match item {
                Ok(None) => continue,
                Ok(Some(f)) if f.bytes.len() > VIEW_FRAME_MAX => {
                    let _ = tx.try_send(event("end", json!({ "reason": "a frame too large to show" })));
                    break;
                }
                Ok(Some(f)) if !f.full && !whole => {
                    let _ = tx.try_send(event("end", json!({ "reason": "the terminal stream did not start with a whole frame" })));
                    break;
                }
                Ok(Some(f)) => {
                    whole = true;
                    tx.try_send(event("frame", json!(f)))
                }
                Err(e) => {
                    let _ = tx.try_send(event("end", json!({ "reason": e.to_string() })));
                    break;
                }
            };
            // A reader that falls behind: its stream ends here, and the page opens a new one.
            if sent.is_err() {
                break;
            }
        }
        let _ = tx.try_send(event("end", json!({ "reason": "the terminal stream ended" })));
        // The frames are dropped here: the runtime's stream stops (its child is killed).
    });
    let stream = ReceiverStream::new(rx).map(Ok::<_, Infallible>);
    Ok(([(header::CACHE_CONTROL, "no-store")], Sse::new(stream).keep_alive(KeepAlive::new().interval(std::time::Duration::from_secs(2)))).into_response())
}
