use axum::extract::Path;

static INDEX_CSS: &str = include_str!("../../assets/index.css");
static INDEX_JS: &str = include_str!("../../assets/index.js");

pub async fn handle_assets(Path(path): Path<String>) -> impl axum::response::IntoResponse {
    let mut headers = http::HeaderMap::new();

    if path == "index.css" {
        headers.insert(http::header::CONTENT_TYPE, "text/css".parse().unwrap());
        (http::StatusCode::OK, headers, INDEX_CSS)
    } else if path == "index.js" {
        headers.insert(
            http::header::CONTENT_TYPE,
            "application/javascript".parse().unwrap(),
        );
        (http::StatusCode::OK, headers, INDEX_JS)
    } else {
        (http::StatusCode::NOT_FOUND, headers, "")
    }
}
