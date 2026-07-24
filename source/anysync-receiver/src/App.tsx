import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { QRCodeSVG } from "qrcode.react";

type ServerStatus = {
  running: boolean;
  save_dir: string | null;
  upload_url: string | null;
  token: string | null;
  port: number | null;
};

type UploadEvent = {
  filename: string;
  saved_path: string;
  size_bytes: number;
  received_at: string;
};

const isTauriRuntime = () => "__TAURI_INTERNALS__" in window;

const tauriInvoke = async <T,>(command: string, args?: Record<string, unknown>) => {
  if (!isTauriRuntime()) {
    throw new Error(
      "This screen must run inside the Tauri desktop window. Start it with `npm run tauri:dev`; do not open the Vite URL in a normal browser."
    );
  }
  return invoke<T>(command, args);
};

const formatBytes = (bytes: number) => {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let index = 0;
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024;
    index += 1;
  }
  return `${value.toFixed(value >= 10 ? 1 : 2)} ${units[index]}`;
};

function App() {
  const [status, setStatus] = useState<ServerStatus>({
    running: false,
    save_dir: null,
    upload_url: null,
    token: null,
    port: null
  });
  const [recentUploads, setRecentUploads] = useState<UploadEvent[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const fullUrl = useMemo(() => {
    if (!status.upload_url || !status.token) return null;
    return `${status.upload_url}/?token=${encodeURIComponent(status.token)}`;
  }, [status.upload_url, status.token]);

  const refreshStatus = async () => {
    const next = await tauriInvoke<ServerStatus>("server_status");
    setStatus(next);
  };

  useEffect(() => {
    refreshStatus().catch((err) => setError(String(err)));
    if (!isTauriRuntime()) return;

    const unlistenPromise = listen<UploadEvent>("upload-complete", (event) => {
      setRecentUploads((items) => [event.payload, ...items].slice(0, 12));
    });

    return () => {
      unlistenPromise.then((unlisten) => unlisten());
    };
  }, []);

  const chooseFolder = async () => {
    setBusy(true);
    setError(null);
    try {
      const dir = await tauriInvoke<string | null>("select_save_dir");
      if (dir) setStatus((current) => ({ ...current, save_dir: dir }));
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const start = async () => {
    setBusy(true);
    setError(null);
    try {
      const next = await tauriInvoke<ServerStatus>("start_server", {
        saveDir: status.save_dir
      });
      setStatus(next);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const stop = async () => {
    setBusy(true);
    setError(null);
    try {
      const next = await tauriInvoke<ServerStatus>("stop_server");
      setStatus(next);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <main className="shell">
      <section className="header">
        <div>
          <p className="eyebrow">LAN Photo Receiver</p>
          <h1>AnySync Receiver</h1>
        </div>
        <div className={status.running ? "status running" : "status"}>
          <span />
          {status.running ? "Receiving" : "Stopped"}
        </div>
      </section>

      <section className="toolbar">
        <div className="folder">
          <span>Save folder</span>
          <strong>{status.save_dir ?? "No folder selected"}</strong>
        </div>
        <button onClick={chooseFolder} disabled={busy || status.running}>
          Change
        </button>
      </section>

      <section className="receiver">
        <div className="connection">
          <p className="label">Phone URL</p>
          <div className="urlBox">{fullUrl ?? "Start receiver to show upload URL"}</div>
          <div className="actions">
            {!status.running ? (
              <button className="primary" onClick={start} disabled={busy}>
                Start Receiver
              </button>
            ) : (
              <button className="danger" onClick={stop} disabled={busy}>
                Stop Receiver
              </button>
            )}
          </div>
          {error && <p className="error">{error}</p>}
        </div>

        <div className="qrPanel">
          {fullUrl ? (
            <QRCodeSVG value={fullUrl} size={190} includeMargin />
          ) : (
            <div className="qrPlaceholder">QR</div>
          )}
        </div>
      </section>

      <section className="uploads">
        <div className="sectionTitle">
          <h2>Recent Uploads</h2>
          <span>{recentUploads.length} file(s)</span>
        </div>
        {recentUploads.length === 0 ? (
          <p className="empty">No uploads yet.</p>
        ) : (
          <div className="uploadList">
            {recentUploads.map((item) => (
              <article className="uploadItem" key={`${item.saved_path}-${item.received_at}`}>
                <div>
                  <strong>{item.filename}</strong>
                  <span>{item.saved_path}</span>
                </div>
                <time>{formatBytes(item.size_bytes)}</time>
              </article>
            ))}
          </div>
        )}
      </section>
    </main>
  );
}

export default App;
