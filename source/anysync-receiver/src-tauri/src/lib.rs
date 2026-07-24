use axum::{
    extract::{DefaultBodyLimit, Multipart, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::{get, post},
    Router,
};
use chrono::Utc;
use local_ip_address::local_ip;
use rand::{distributions::Alphanumeric, Rng};
use serde::{Deserialize, Serialize};
use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, Emitter};
use tokio::{io::AsyncWriteExt, sync::oneshot};
use tower_http::cors::CorsLayer;

const DEFAULT_PORT: u16 = 8787;

#[derive(Debug, Clone, Serialize)]
struct ServerStatus {
    running: bool,
    save_dir: Option<String>,
    upload_url: Option<String>,
    token: Option<String>,
    port: Option<u16>,
}

#[derive(Debug, Clone, Serialize)]
struct UploadEvent {
    filename: String,
    saved_path: String,
    size_bytes: u64,
    received_at: String,
}

#[derive(Default)]
struct AppState {
    server: Option<ServerHandle>,
    save_dir: Option<PathBuf>,
}

struct ServerHandle {
    upload_url: String,
    token: String,
    port: u16,
    shutdown: Option<oneshot::Sender<()>>,
}

#[derive(Clone)]
struct UploadState {
    save_dir: PathBuf,
    token: String,
    app: AppHandle,
}

#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

#[tauri::command]
fn select_save_dir() -> Option<String> {
    rfd::FileDialog::new()
        .set_title("Select AnySync save folder")
        .pick_folder()
        .map(|path| path.to_string_lossy().to_string())
}

#[tauri::command]
fn server_status(state: tauri::State<'_, Arc<Mutex<AppState>>>) -> ServerStatus {
    status_from_state(&state.lock().expect("state poisoned"))
}

#[tauri::command]
fn start_server(
    save_dir: Option<String>,
    app: AppHandle,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<ServerStatus, String> {
    let mut guard = state.lock().map_err(|_| "state lock failed".to_string())?;
    if guard.server.is_some() {
        return Ok(status_from_state(&guard));
    }

    let dir = match save_dir {
        Some(value) if !value.trim().is_empty() => PathBuf::from(value),
        _ => default_save_dir(),
    };
    std::fs::create_dir_all(&dir).map_err(|err| format!("failed to create save folder: {err}"))?;

    let token = random_token();
    let port = find_available_port(DEFAULT_PORT)?;
    let ip = local_ip().map_err(|err| format!("failed to get LAN IP: {err}"))?;
    let upload_url = format!("http://{ip}:{port}");
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let upload_state = UploadState {
        save_dir: dir.clone(),
        token: token.clone(),
        app,
    };

    std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().expect("failed to start tokio runtime");
        runtime.block_on(async move {
            if let Err(err) = run_http_server(port, upload_state, shutdown_rx).await {
                eprintln!("AnySync HTTP server stopped: {err}");
            }
        });
    });

    guard.save_dir = Some(dir);
    guard.server = Some(ServerHandle {
        upload_url,
        token,
        port,
        shutdown: Some(shutdown_tx),
    });

    Ok(status_from_state(&guard))
}

#[tauri::command]
fn stop_server(state: tauri::State<'_, Arc<Mutex<AppState>>>) -> Result<ServerStatus, String> {
    let mut guard = state.lock().map_err(|_| "state lock failed".to_string())?;
    if let Some(mut server) = guard.server.take() {
        if let Some(shutdown) = server.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
    Ok(status_from_state(&guard))
}

async fn run_http_server(
    port: u16,
    upload_state: UploadState,
    shutdown_rx: oneshot::Receiver<()>,
) -> Result<(), String> {
    let router = Router::new()
        .route("/", get(upload_page))
        .route("/api/upload", post(upload_file))
        .layer(DefaultBodyLimit::disable())
        .layer(CorsLayer::permissive())
        .with_state(upload_state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|err| format!("failed to bind port {port}: {err}"))?;

    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = shutdown_rx.await;
        })
        .await
        .map_err(|err| err.to_string())
}

async fn upload_page(Query(query): Query<TokenQuery>) -> impl IntoResponse {
    Html(upload_page_html(&query.token.unwrap_or_default()))
}

async fn upload_file(
    State(state): State<UploadState>,
    Query(query): Query<TokenQuery>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    if query.token.as_deref() != Some(state.token.as_str()) {
        return (StatusCode::UNAUTHORIZED, "invalid token").into_response();
    }

    while let Some(field_result) = multipart.next_field().await.transpose() {
        let mut field = match field_result {
            Ok(field) => field,
            Err(err) => {
                return (
                    StatusCode::BAD_REQUEST,
                    format!("failed to read multipart field: {err}"),
                )
                    .into_response()
            }
        };

        let Some(original_name) = field.file_name().map(ToOwned::to_owned) else {
            continue;
        };

        let safe_name = sanitize_filename::sanitize(&original_name);
        let target_path = unique_path(&state.save_dir, &safe_name);
        let mut file = match tokio::fs::File::create(&target_path).await {
            Ok(file) => file,
            Err(err) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("failed to create file: {err}"),
                )
                .into_response()
            }
        };

        let mut size_bytes = 0_u64;
        loop {
            match field.chunk().await {
                Ok(Some(chunk)) => {
                    size_bytes += chunk.len() as u64;
                    if let Err(err) = file.write_all(&chunk).await {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("failed to write file: {err}"),
                        )
                        .into_response();
                    }
                }
                Ok(None) => break,
                Err(err) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        format!("failed to read upload chunk: {err}"),
                    )
                    .into_response()
                }
            }
        }

        let event = UploadEvent {
            filename: original_name,
            saved_path: target_path.to_string_lossy().to_string(),
            size_bytes,
            received_at: Utc::now().to_rfc3339(),
        };
        let _ = state.app.emit("upload-complete", event);
    }

    (StatusCode::OK, "ok").into_response()
}

fn status_from_state(state: &AppState) -> ServerStatus {
    match &state.server {
        Some(server) => ServerStatus {
            running: true,
            save_dir: state
                .save_dir
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            upload_url: Some(server.upload_url.clone()),
            token: Some(server.token.clone()),
            port: Some(server.port),
        },
        None => ServerStatus {
            running: false,
            save_dir: state
                .save_dir
                .as_ref()
                .map(|path| path.to_string_lossy().to_string()),
            upload_url: None,
            token: None,
            port: None,
        },
    }
}

fn default_save_dir() -> PathBuf {
    dirs::picture_dir()
        .or_else(dirs::download_dir)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("AnySync")
}

fn random_token() -> String {
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(8)
        .map(char::from)
        .collect()
}

fn find_available_port(start: u16) -> Result<u16, String> {
    for port in start..start + 80 {
        if std::net::TcpListener::bind(("0.0.0.0", port)).is_ok() {
            return Ok(port);
        }
    }
    Err("no available port found".to_string())
}

fn unique_path(dir: &Path, filename: &str) -> PathBuf {
    let candidate = dir.join(filename);
    if !candidate.exists() {
        return candidate;
    }

    let path = Path::new(filename);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("upload");
    let ext = path.extension().and_then(|value| value.to_str());

    for index in 1..10_000 {
        let next_name = match ext {
            Some(ext) if !ext.is_empty() => format!("{stem} ({index}).{ext}"),
            _ => format!("{stem} ({index})"),
        };
        let next = dir.join(next_name);
        if !next.exists() {
            return next;
        }
    }

    dir.join(format!("upload-{}.bin", Utc::now().timestamp_millis()))
}

fn upload_page_html(token: &str) -> String {
    format!(
        r##"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>AnySync Upload</title>
  <style>
    body {{ background: #f6f7f9; color: #18202d; font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; margin: 0; padding: 24px; }}
    main {{ margin: 0 auto; max-width: 560px; }}
    h1 {{ font-size: 30px; margin: 0 0 18px; }}
    .panel {{ background: #fff; border: 1px solid #dfe4ec; border-radius: 8px; padding: 18px; }}
    input {{ border: 1px solid #ccd4df; border-radius: 8px; display: block; margin: 14px 0; padding: 13px; width: 100%; }}
    button {{ background: #1769e0; border: 0; border-radius: 8px; color: #fff; font-size: 16px; font-weight: 700; min-height: 46px; width: 100%; }}
    button:disabled {{ opacity: .55; }}
    #log {{ color: #526177; line-height: 1.5; margin-top: 14px; white-space: pre-wrap; }}
  </style>
</head>
<body>
  <main>
    <h1>AnySync</h1>
    <section class="panel">
      <strong>Send original photos or videos to this PC</strong>
      <input id="files" type="file" accept="image/*,video/*,.jpg,.jpeg,.png,.gif,.webp,.heic,.heif,.mp4,.mov,.m4v,.avi,.mkv,.webm,.3gp" multiple>
      <button id="upload">Upload</button>
      <div id="log"></div>
    </section>
  </main>
  <script>
    const token = "{token}";
    const input = document.querySelector("#files");
    const button = document.querySelector("#upload");
    const log = document.querySelector("#log");
    let selectedFiles = [];

    const uploadSelectedFiles = async () => {{
      const files = selectedFiles.length ? selectedFiles : Array.from(input.files || []);
      if (!files.length) {{
        log.textContent = "No file was selected. If you are using an in-app browser, open this page in Safari or Chrome and try again.";
        return;
      }}

      button.disabled = true;
      log.textContent = `Preparing to upload ${{files.length}} file(s)...`;

      try {{
        const form = new FormData();
        for (const file of files) {{
          form.append("files", file, file.name);
        }}
        await new Promise((resolve, reject) => {{
          const xhr = new XMLHttpRequest();
          xhr.open("POST", `/api/upload?token=${{encodeURIComponent(token)}}`);
          xhr.upload.onprogress = (event) => {{
            if (!event.lengthComputable) return;
            const percent = Math.round((event.loaded / event.total) * 100);
            log.textContent = `Uploading ${{files.length}} file(s)... ${{percent}}%`;
          }};
          xhr.onload = () => {{
            if (xhr.status >= 200 && xhr.status < 300) resolve();
            else reject(xhr.responseText || xhr.statusText);
          }};
          xhr.onerror = () => reject("network error");
          xhr.send(form);
        }});
        log.textContent = `Upload complete: ${{files.length}} file(s).`;
        selectedFiles = [];
        input.value = "";
      }} catch (error) {{
        log.textContent = `Upload failed: ${{error}}`;
      }} finally {{
        button.disabled = false;
      }}
    }};

    input.addEventListener("click", () => {{
      selectedFiles = [];
      input.value = "";
      log.textContent = "";
    }});

    input.addEventListener("change", () => {{
      selectedFiles = Array.from(input.files || []);
      if (!selectedFiles.length) {{
        log.textContent = "No file was selected. Try choosing from Files instead of Photos, or open this page in Safari/Chrome.";
        return;
      }}
      log.textContent = `Selected ${{selectedFiles.length}} file(s). Uploading now...`;
      uploadSelectedFiles();
    }});

    button.addEventListener("click", uploadSelectedFiles);
  </script>
</body>
</html>"##
    )
}

pub fn run() {
    tauri::Builder::default()
        .manage(Arc::new(Mutex::new(AppState::default())))
        .invoke_handler(tauri::generate_handler![
            select_save_dir,
            server_status,
            start_server,
            stop_server
        ])
        .run(tauri::generate_context!())
        .expect("error while running AnySync Receiver");
}
