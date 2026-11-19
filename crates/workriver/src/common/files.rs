// Files as both sides handle them: the type a file is sent and served as.

/// The type to serve a file as: the given one, else guessed from the name.
pub fn mime(name: &str, given: Option<&str>) -> String {
    match given.map(str::trim).filter(|t| !t.is_empty() && *t != "application/octet-stream") {
        Some(t) => t.to_string(),
        None => mime_guess::from_path(name).first_or_octet_stream().to_string(),
    }
}
