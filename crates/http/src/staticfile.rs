//! Static files from a directory, as Go's `http.ServeContent` serves them: content type by extension,
//! `Last-Modified`, and `304 Not Modified` for a matching `If-Modified-Since`.
use crate::util::{http_date, parse_http_date};
use crate::{Body, Request, Response};
use std::path::{Path, PathBuf};

/// Content type for a file name, from its extension.
pub fn content_type(name: &str) -> &'static str {
    let ext = match name.rsplit_once('.') {
        Some((_, e)) => e,
        None => "",
    };
    match ext.to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/vnd.microsoft.icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "eot" => "application/vnd.ms-fontobject",
        "txt" | "md" => "text/plain; charset=utf-8",
        "xml" => "text/xml; charset=utf-8",
        "wasm" => "application/wasm",
        "pdf" => "application/pdf",
        "yaml" | "yml" => "application/yaml",
        _ => "application/octet-stream",
    }
}

/// Joins a decoded relative path under `root`, refusing `..`, empty-name tricks and hidden traversal.
pub fn safe_join(root: &Path, rel: &str) -> Option<PathBuf> {
    let mut p = root.to_path_buf();
    for seg in rel.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." || seg.contains('\\') || seg.contains('\0') {
            return None;
        }
        p.push(seg);
    }
    Some(p)
}

/// Serves `rel` (already decoded) under `root`. None when there is no such regular file.
/// `cache_control` is set when not empty.
pub fn serve(req: &Request, root: &Path, rel: &str, cache_control: &str) -> Option<Response> {
    let path = safe_join(root, rel)?;
    let f = std::fs::File::open(&path).ok()?;
    let meta = f.metadata().ok()?;
    if !meta.is_file() {
        return None;
    }
    let mtime = match meta.modified() {
        Ok(t) => match t.duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => d.as_secs() as i64,
            Err(_) => 0,
        },
        Err(_) => 0,
    };
    let mut resp = Response::new(200);
    if !cache_control.is_empty() {
        resp.headers.push(("Cache-Control".to_string(), cache_control.to_string()));
    }
    if mtime > 0 {
        resp.headers.push(("Last-Modified".to_string(), http_date(mtime)));
        if let Some(ims) = req.header("if-modified-since") {
            if let Some(t) = parse_http_date(ims) {
                if mtime <= t {
                    resp.status = 304;
                    return Some(resp);
                }
            }
        }
    }
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    resp.headers.push(("Content-Type".to_string(), content_type(&name).to_string()));
    resp.body = Body::File(f, meta.len());
    Some(resp)
}
