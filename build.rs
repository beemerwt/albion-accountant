use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

fn main() {
    println!("cargo:rerun-if-changed=webapp/dist");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR should be set"));
    let generated = out_dir.join("webapp_assets.rs");
    let dist = Path::new("webapp/dist");

    let mut file = fs::File::create(generated).expect("failed to create generated webapp assets");
    if dist.exists() {
        let assets = collect_assets(dist).expect("failed to collect webapp assets");
        write_assets(&mut file, &assets).expect("failed to write generated webapp assets");
    } else {
        write_fallback(&mut file).expect("failed to write fallback webapp assets");
    }
}

fn collect_assets(root: &Path) -> io::Result<Vec<(String, String, String)>> {
    let mut assets = Vec::new();
    collect_assets_from(root, root, &mut assets)?;
    assets.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(assets)
}

fn collect_assets_from(
    root: &Path,
    dir: &Path,
    assets: &mut Vec<(String, String, String)>,
) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_assets_from(root, &path, assets)?;
            continue;
        }

        let relative = path
            .strip_prefix(root)
            .expect("asset should be under root")
            .to_string_lossy()
            .replace('\\', "/");
        let route = format!("/{relative}");
        let mime = mime_for_path(&path).to_string();
        let source_path = fs::canonicalize(&path)?.display().to_string();
        assets.push((route, mime, source_path));
    }
    Ok(())
}

fn write_assets(file: &mut fs::File, assets: &[(String, String, String)]) -> io::Result<()> {
    writeln!(
        file,
        "pub(crate) const WEBAPP_ASSETS: &[(&str, &str, &[u8])] = &["
    )?;
    for (route, mime, source_path) in assets {
        writeln!(
            file,
            "    ({route:?}, {mime:?}, include_bytes!({source_path:?})),"
        )?;
    }
    writeln!(file, "];")
}

fn write_fallback(file: &mut fs::File) -> io::Result<()> {
    writeln!(
        file,
        "pub(crate) const WEBAPP_ASSETS: &[(&str, &str, &[u8])] = &[(\"/index.html\", \"text/html; charset=utf-8\", {:?}.as_bytes())];",
        fallback_html()
    )
}

fn fallback_html() -> &'static str {
    r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1.0" />
  <title>Albion Accountant</title>
  <style>
    body { margin: 0; font-family: system-ui, sans-serif; background: #15171c; color: #f7f1e3; display: grid; min-height: 100vh; place-items: center; }
    main { max-width: 680px; padding: 32px; }
    code { color: #f3c969; }
  </style>
</head>
<body>
  <main>
    <h1>Albion Accountant</h1>
    <p>The Rust webserver is running. Build the React app with <code>cd webapp && npm install && npm run build</code>, then rebuild the Rust binary to embed the UI.</p>
  </main>
</body>
</html>"#
}

fn mime_for_path(path: &Path) -> &'static str {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("css") => "text/css; charset=utf-8",
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("txt") => "text/plain; charset=utf-8",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}
