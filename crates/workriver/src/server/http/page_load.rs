// The signed-in page's first load, in navigation-relative milliseconds and encoded bytes.
use super::*;
use crate::common::diagnostics::PageApiTiming;
use std::collections::BTreeSet;

const API_PATHS: &str = include_str!("../../../../../frontend/web/src/api/page-load-paths.txt");
const MAX_REQUESTS: u64 = 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PageLoad {
    ms: u64,
    #[serde(default, deserialize_with = "number")]
    js_ms: Option<u64>,
    #[serde(default, deserialize_with = "number")]
    css_ms: Option<u64>,
    #[serde(default, deserialize_with = "number")]
    request_ms: Option<u64>,
    #[serde(default, deserialize_with = "number")]
    response_ms: Option<u64>,
    #[serde(default, deserialize_with = "number")]
    bytes: Option<u64>,
    ready_ms: u64,
    #[serde(default, deserialize_with = "number")]
    parse_ms: Option<u64>,
    app_ms: u64,
    render_ms: u64,
    git_pending: u8,
    build: u64,
    api: Vec<PageApiTiming>,
}

// Absent resource timings are omitted. An explicitly supplied value must be numeric.
fn number<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    u64::deserialize(d).map(Some)
}

pub(super) async fn record(State(app): State<Shared>, Extension(caller): Extension<Caller>, Json(p): Json<PageLoad>) -> Result<StatusCode, ApiError> {
    if !caller.session {
        return Err(ApiError(StatusCode::UNAUTHORIZED, "a page session is required".into()));
    }
    app.user(&caller)?;
    if p.ms > 3_600_000
        || [p.js_ms, p.css_ms, p.request_ms, p.response_ms, Some(p.ready_ms)].into_iter().flatten().any(|ms| ms > p.ms)
        || p.parse_ms.is_some_and(|ms| ms > p.ready_ms)
        || p.ms.checked_sub(p.ready_ms).and_then(|ms| ms.checked_sub(p.app_ms)) != Some(p.render_ms)
        || p.bytes.is_some_and(|bytes| bytes > 512 * 1024 * 1024)
        || p.git_pending > 1
        || p.build >= 1 << 48
    {
        return Err(ApiError(StatusCode::UNPROCESSABLE_ENTITY, "page load timing is out of range".into()));
    }
    let mut seen = BTreeSet::new();
    if p.api.iter().any(|a| {
        !API_PATHS.lines().any(|path| path == a.path) || !seen.insert(a.path.as_str())
            || !(1..=MAX_REQUESTS).contains(&a.count) || a.max_ms > p.ms
            || a.ms < a.max_ms || a.ms > a.max_ms * a.count
    }) || p.api.iter().map(|a| a.count).sum::<u64>() > MAX_REQUESTS {
        return Err(ApiError(StatusCode::UNPROCESSABLE_ENTITY, "page API timing is out of range".into()));
    }
    let api = serde_json::to_string(&p.api).expect("API timing rows serialize");
    tracing::info!(target: "perf", kind = "page_load", ms = p.ms, js_ms = p.js_ms, css_ms = p.css_ms,
        request_ms = p.request_ms, response_ms = p.response_ms, bytes = p.bytes, ready_ms = p.ready_ms,
        parse_ms = p.parse_ms, app_ms = p.app_ms, render_ms = p.render_ms,
        git_pending = p.git_pending, build = p.build, api = %api);
    Ok(StatusCode::NO_CONTENT)
}
