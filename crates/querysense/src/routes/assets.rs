use axum::extract::Path;
use axum::response::{IntoResponse, Response};
use http::{HeaderMap, StatusCode, header};

static STYLES_CSS: &str = include_str!("../../../../dist/styles.css");
static MAIN_JS: &str = include_str!("../../../../dist/main.js");

pub async fn handle_assets(Path(path): Path<String>) -> Response {
    match path.as_str() {
        "styles.css" => {
            let mut headers = HeaderMap::new();
            headers.insert(header::CONTENT_TYPE, "text/css".parse().unwrap());
            (StatusCode::OK, headers, STYLES_CSS).into_response()
        }
        "main.js" => {
            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                "application/javascript".parse().unwrap(),
            );
            (StatusCode::OK, headers, MAIN_JS).into_response()
        }
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}
