// The page's session: whether there is one, a one-time code for it, the code exchanged.
use super::*;

/// Whether this request is the user's; the page shows nothing else until it is.
pub(super) async fn session(State(app): State<Shared>, Extension(caller): Extension<Caller>) -> Json<serde_json::Value> {
    Json(json!({ "user": app.user(&caller).is_ok() }))
}

#[derive(Deserialize)]
pub(super) struct Code {
    code: String,
}

/// A one-time code for a session: the cookie is the page's alone (HttpOnly) and is not
/// sent along with requests from other sites (SameSite=Strict).
pub(super) async fn session_start(State(app): State<Shared>, Json(c): Json<Code>) -> Result<Response, ApiError> {
    let value = app.auth.redeem(&c.code).ok_or_else(|| ApiError(StatusCode::UNAUTHORIZED, "this link was used already or is older than 5 minutes; run `workriver open` for a new one".into()))?;
    let cookie = format!("{}={value}; HttpOnly; SameSite=Strict; Path=/; Max-Age={}", auth::COOKIE, auth::SESSION_DAYS * 24 * 3600);
    Ok(([(header::SET_COOKIE, cookie)], Json(json!({ "user": true }))).into_response())
}

/// A new one-time code, for `workriver open`: the user's to ask for.
#[derive(Deserialize, Default)]
pub(super) struct CodeFor {
    /// The link will be printed in a terminal instead of handed to the browser.
    #[serde(default)]
    printed: bool,
}

pub(super) async fn session_code(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(c): Json<CodeFor>) -> Result<Json<serde_json::Value>, ApiError> {
    app.user(&caller)?;
    let code = app.auth.code(c.printed).map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(json!({ "url": format!("{}/#k={code}", app.page()) })))
}
