//! The console's files, built into the program so the image stays one
//! file.

use bytes::Bytes;
use hyper::StatusCode;

use crate::server::{Body, Response};

/// A script of the page, built in.
macro_rules! script {
    ($name:literal) => {
        (
            $name,
            "text/javascript; charset=utf-8",
            include_str!(concat!("../../console/", $name)),
        )
    };
}

/// Each file: its name under the console's path, its type and content.
const FILES: &[(&str, &str, &str)] = &[
    (
        "",
        "text/html; charset=utf-8",
        include_str!("../../console/index.html"),
    ),
    (
        "console.css",
        "text/css; charset=utf-8",
        include_str!("../../console/console.css"),
    ),
    script!("console.js"),
    script!("catalog.js"),
    script!("service.js"),
    script!("requests.js"),
    script!("home.js"),
    script!("s3.js"),
];

/// The file at `name`, the part of the path after the console's.
pub fn serve(name: &str) -> Response {
    let name = if name == "index.html" { "" } else { name };
    let response = hyper::Response::builder();
    match FILES.iter().find(|(file, ..)| *file == name) {
        Some((_, kind, content)) => response
            .header("content-type", *kind)
            // Always the emulator's own version of the page.
            .header("cache-control", "no-cache")
            .body(Body::full(Bytes::from_static(content.as_bytes()))),
        None => response
            .status(StatusCode::NOT_FOUND)
            .header("content-type", "text/plain; charset=utf-8")
            .body(Body::full(Bytes::from(format!("no console file {name}")))),
    }
    .expect("a valid response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_its_files() {
        assert_eq!(serve("").status(), StatusCode::OK);
        assert_eq!(serve("index.html").status(), StatusCode::OK);
        assert_eq!(serve("console.js").status(), StatusCode::OK);
        assert_eq!(
            serve("console.js").headers()["content-type"],
            "text/javascript; charset=utf-8"
        );
        assert_eq!(serve("../secrets").status(), StatusCode::NOT_FOUND);
    }
}
