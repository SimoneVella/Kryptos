import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";

// `npm run dev` opened in a normal browser: fake the backend for design work.
if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
  await import("./lib/mock");
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
