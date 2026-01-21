import React from "react";
import ReactDOM from "react-dom/client";
import "./index.css";
import { mountHalftoneBackground } from "./lib/halftoneBackground";
import Scrutinizer from "./Scrutinizer";

const rootEl = document.getElementById("root");
if (!rootEl) {
  throw new Error("Missing #root element");
}

mountHalftoneBackground();

ReactDOM.createRoot(rootEl).render(
  <React.StrictMode>
    <Scrutinizer />
  </React.StrictMode>,
);
