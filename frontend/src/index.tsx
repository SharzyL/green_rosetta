import React from "react";
import ReactDOM from "react-dom/client";
import "./index.css";
import App from "./App";
import { mountHalftoneBackground } from "./lib/halftoneBackground";

const rootEl = document.getElementById("root");
if (!rootEl) {
	throw new Error("Missing #root element");
}

mountHalftoneBackground();

ReactDOM.createRoot(rootEl).render(
	<React.StrictMode>
		<App />
	</React.StrictMode>,
);
