import React from "react";
import ReactDOM from "react-dom/client";
import "./styles.css";

function App() {
  return (
    <main>
      <p className="eyebrow">ANNIVERSARY / TBC CLASSIC</p>
      <h1>WoW Coach</h1>
      <p className="lede">
        A local-first companion for turning addon snapshots into useful character history.
      </p>
      <section aria-labelledby="status-title">
        <h2 id="status-title">Baseline status</h2>
        <ul>
          <li>Collector schema and safe character snapshot fields</li>
          <li>Collision-safe shared Rust identity model</li>
          <li>Local SQLite history migration</li>
        </ul>
        <p className="notice">
          SavedVariables import and coaching views are intentionally not wired in this first baseline.
        </p>
      </section>
    </main>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
