import { MotionProvider } from "@bug-on/m3-expressive/core";
import { LazyMotion, domMax } from "motion/react";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";

const boot =
  import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)
    ? import("./lib/devMock").then((m) => m.installDevMock())
    : Promise.resolve();

/*
 * The library's components animate with `m.*` elements, which stay frozen at their initial state
 * (a menu panel at opacity 0, scale 0.8) unless a LazyMotion provider supplies the animation
 * features. MD3ThemeProvider normally does that; this app themes itself through theme.css instead
 * of the provider's color engine, so it supplies the same two wrappers directly.
 */
void boot.then(() =>
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <LazyMotion features={domMax}>
      <MotionProvider motion="auto">
        <App />
      </MotionProvider>
    </LazyMotion>
  </React.StrictMode>,
),
);
