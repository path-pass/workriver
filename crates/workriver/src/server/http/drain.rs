// What a handler leaves of a request's body: read and thrown away after it has answered,
// up to a cap and for a while. Unread, the connection would be closed with the client still
// sending, which resets it and loses the answer (hyper reads only what has arrived of a
// body nobody reads, then closes): the client of a refusal from the headers alone, or of a
// record larger than taken, would hear only that the connection broke.
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use axum::body::{Body, Bytes, HttpBody};

use super::*;

/// The most the service reads of what a handler left: the largest body the CLI sends (an
/// attachment; it refuses a larger file before sending), so every request of the CLI gets
/// its answer. It is thrown away frame by frame as it comes, so it costs time, never memory,
/// and no more than an upload that is taken (which keeps all of it). A body that has more
/// left, or is still coming after `LEFT_BODY_SECS`, is left: its connection is closed and
/// the client sees it closed.
const MOST: u64 = crate::common::wire::MAX_ATTACHMENT as u64;

/// A request's body as the handler reads it; what it does not read stays here.
struct Kept(Arc<Mutex<Body>>);

impl HttpBody for Kept {
    type Data = Bytes;
    type Error = axum::Error;

    fn poll_frame(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<http_body::Frame<Bytes>, axum::Error>>> {
        Pin::new(&mut *self.0.lock().unwrap()).poll_frame(cx)
    }

    fn is_end_stream(&self) -> bool {
        self.0.lock().unwrap().is_end_stream()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        self.0.lock().unwrap().size_hint()
    }
}

/// Around everything else, on every listener: the answer goes out once the rest of the body
/// is read, or given up on.
pub(super) async fn drain(req: Request, next: Next) -> Response {
    let (parts, body) = req.into_parts();
    let kept = Arc::new(Mutex::new(body));
    let res = next.run(Request::from_parts(parts, Body::new(Kept(kept.clone())))).await;
    let mut rest = std::mem::take(&mut *kept.lock().unwrap());
    // What is left, as far as the request says (its Content-Length less what was read).
    if rest.is_end_stream() || rest.size_hint().lower() > MOST {
        return res;
    }
    let until = tokio::time::Instant::now() + std::time::Duration::from_secs(crate::common::config::defaults::LEFT_BODY_SECS);
    let mut read = 0u64;
    while read <= MOST {
        match tokio::time::timeout_at(until, std::future::poll_fn(|cx| Pin::new(&mut rest).poll_frame(cx))).await {
            Ok(Some(Ok(frame))) => read += frame.data_ref().map_or(0, |d| d.len() as u64),
            // Its end, a broken connection, or the time is up.
            _ => break,
        }
    }
    res
}
