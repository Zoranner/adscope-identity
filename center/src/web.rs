use std::{env, path::PathBuf};

use tower_http::services::{ServeDir, ServeFile};

pub(crate) fn default_web_root() -> PathBuf {
    if let Ok(path) = env::var("ADSS_WEB_ROOT") {
        return PathBuf::from(path);
    }

    let runtime_web = PathBuf::from("web");
    if runtime_web.join("index.html").is_file() {
        return runtime_web;
    }

    let source_web = PathBuf::from("center")
        .join("web")
        .join(".output")
        .join("public");
    if source_web.join("index.html").is_file() {
        return source_web;
    }

    runtime_web
}

pub(crate) fn static_service(root: impl Into<PathBuf>) -> ServeDir<ServeFile> {
    let root = root.into();
    ServeDir::new(&root)
        .append_index_html_on_directories(true)
        .fallback(ServeFile::new(root.join("index.html")))
}
