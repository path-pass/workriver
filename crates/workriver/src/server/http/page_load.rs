// The signed-in page's first load, in milliseconds and encoded bytes.
use super::*;
use crate::common::diagnostics::PageApiTiming;

const API_PATHS: &str = include_str!("../../../../../frontend/web/src/api/page-load-paths.txt");
const MAX_MS: u64 = 3_600_000;
const MAX_REQUESTS: u64 = 1024;
const MAX_API_TOTAL_MS: u64 = MAX_MS * MAX_REQUESTS;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PageLoad {
    ms: u64,
    js_ms: Option<u64>,
    css_ms: Option<u64>,
    request_ms: Option<u64>,
    response_ms: Option<u64>,
    bytes: Option<u64>,
    ready_ms: u64,
    parse_ms: Option<u64>,
    app_ms: u64,
    render_ms: u64,
    git_pending: u8,
    api: Vec<PageApiTiming>,
}

pub(super) async fn record(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(p): Json<PageLoad>) -> Result<StatusCode, ApiError> {
    if !caller.session {
        return Err(ApiError(StatusCode::UNAUTHORIZED, "a page session is required".into()));
    }
    app.user(&caller)?;
    let times = [Some(p.ms), p.js_ms, p.css_ms, p.request_ms, p.response_ms, Some(p.ready_ms), p.parse_ms, Some(p.app_ms), Some(p.render_ms)];
    if times.into_iter().flatten().any(|ms| ms > MAX_MS)
        || p.bytes.is_some_and(|bytes| bytes > 512 * 1024 * 1024)
        || p.git_pending > 1
    {
        return Err(ApiError(StatusCode::UNPROCESSABLE_ENTITY, "page load timing is out of range".into()));
    }
    if p.api.iter().any(|a| {
        !API_PATHS.lines().any(|path| path == a.path)
            || a.count > MAX_REQUESTS || a.max_ms > MAX_MS || a.ms > MAX_API_TOTAL_MS
    }) {
        return Err(ApiError(StatusCode::UNPROCESSABLE_ENTITY, "page API timing is out of range".into()));
    }
    let api = serde_json::to_string(&p.api).expect("API timing rows serialize");
    tracing::info!(target: "perf", kind = "page_load", ms = p.ms, js_ms = p.js_ms, css_ms = p.css_ms,
        request_ms = p.request_ms, response_ms = p.response_ms, bytes = p.bytes, ready_ms = p.ready_ms,
        parse_ms = p.parse_ms, app_ms = p.app_ms, render_ms = p.render_ms,
        git_pending = p.git_pending, build = crate::common::BUILD, api = %api);
    Ok(StatusCode::NO_CONTENT)
}
