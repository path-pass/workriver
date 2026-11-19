// What every request passes before its handler: the Host and Origin it names, and what
// shows who sends it (the proof, the place it names, the token or the page's session).
use super::*;

/// Which listener a request came in on: this machine's, or a followed machine's own (the
/// far end of its tunnel), which is how the daemon knows where a caller is, whatever it says.
#[derive(Clone)]
pub(in crate::server) struct Via {
    pub(in crate::server) machine: Option<String>,
    /// The port the caller used: ours, or the machine's port that leads here.
    pub(in crate::server) port: u16,
    /// The listener's own port.
    pub(in crate::server) local: u16,
}

/// Only this machine's browser and CLI, by name: the Host must be 127.0.0.1 or localhost
/// with our port (against DNS rebinding), and a POST from a page must come from our origin.
/// On a followed machine's listener the port is the one its CLI uses over there.
///
/// Also works out what the request shows of its sender: the user token; a session cookie,
/// which counts only from our own page; the proof of its process (`x-workriver-caller`,
/// from the caller socket), or on a machine's listener the process id it gives. Against a
/// forged request from another site we rely on the cookie being SameSite=Strict (the browser
/// does not send it along) and, for anything but a GET, on the Origin header being there and
/// ours.
pub(super) async fn guard(State(app): State<Shared>, Extension(via): Extension<Via>, mut req: Request, next: Next) -> Response {
    let ours = [via.port, via.local].map(|p| [format!("127.0.0.1:{p}"), format!("localhost:{p}")]).concat();
    // Through a machine's tunnel the port over there is new with every connection: there,
    // any port of 127.0.0.1 or localhost (a name that resolves elsewhere is still refused).
    let fits = |h: &str| ours.iter().any(|o| o == h) || (via.machine.is_some() && h.rsplit_once(':').is_some_and(|(n, p)| matches!(n, "127.0.0.1" | "localhost") && p.parse::<u16>().is_ok()));
    let (allowed, token, session) = {
        let header = |h| req.headers().get(h).map(|v| v.to_str().unwrap_or(""));
        let host_ok = header(header::HOST).is_some_and(fits);
        let our_origin = header(header::ORIGIN).map(|o| o.strip_prefix("http://").is_some_and(fits));
        let get = req.method() == Method::GET;
        let origin_ok = get || our_origin.unwrap_or(true);
        let session = app.auth.session(header(header::COOKIE)) && (get || our_origin == Some(true));
        let here = via.machine.is_none();
        (host_ok && origin_ok, here && app.auth.bearer(header(header::AUTHORIZATION)), here && session)
    };
    if !allowed {
        return (StatusCode::FORBIDDEN, "workriver answers only on 127.0.0.1 or localhost, from its own page").into_response();
    }
    let (place, chain, pid) = {
        let text = |h: &str| req.headers().get(h).and_then(|v| v.to_str().ok()).filter(|p| !p.is_empty()).map(str::to_string);
        let (chain, pid) = match (&via.machine, text("x-workriver-caller")) {
            (Some(_), _) => (None, text("x-workriver-pid").and_then(|p| p.parse().ok())),
            (None, None) => (None, None),
            (None, Some(proof)) => match app.callers.chain(&proof) {
                Some(chain) => (Some(chain), None),
                // A proof stands for thirty seconds; a command that lasts longer fetches another.
                None => return (StatusCode::UNAUTHORIZED, [("x-workriver-caller", crate::common::wire::EXPIRED)], "the proof of the calling process has expired; the `workriver` CLI fetches a new one and tries again").into_response(),
            },
        };
        (text("x-workriver-place"), chain, pid)
    };
    req.extensions_mut().insert(Caller { machine: via.machine, token, session, place, chain, pid });
    next.run(req).await
}
