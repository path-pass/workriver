// The page: the web build embedded in the binary.
use super::*;

#[derive(rust_embed::Embed)]
#[folder = "../../frontend/web/dist/"]
#[allow_missing = true]
struct Ui;

pub(super) async fn ui(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    // Any other path is a page of the app: index.html.
    let asset = Ui::get(path).filter(|_| !path.is_empty());
    // Built files carry their content's hash in their name: kept for good. The page itself
    // names the current ones, so it is asked for again every time.
    let cache = if asset.is_some() && path.starts_with("assets/") { "public, max-age=31536000, immutable" } else { "no-cache" };
    match asset.or_else(|| Ui::get("index.html")) {
        Some(f) => ([(header::CONTENT_TYPE, f.metadata.mimetype().to_string()), (header::CACHE_CONTROL, cache.to_string())], f.data).into_response(),
        None => (StatusCode::NOT_FOUND, "The web UI is not built: run `npm run build` in frontend/web, then build workriver again.").into_response(),
    }
}
