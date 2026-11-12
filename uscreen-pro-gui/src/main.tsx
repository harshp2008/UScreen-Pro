/*
 * Entry point: mounts <App/>. Outside Tauri in development it installs the in-browser mock of the Tauri IPC first (src/lib/devMock.ts).
 */
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";

/** In a plain browser (no Tauri) `vite dev` installs the mock backend first; inside the app nothing is waited for. */
const boot =
  import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)
    ? import("./lib/devMock").then((m) => m.installDevMock())
    : Promise.resolve();

void boot.then(() =>
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  ),
);
