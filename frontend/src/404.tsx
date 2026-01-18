import React from "react";
import ReactDOM from "react-dom/client";
import "./index.css";
import { mountHalftoneBackground } from "./lib/halftoneBackground";
import OutsideNow from "./OutsideNow";

const rootEl = document.getElementById("root");
if (!rootEl) {
	throw new Error("Missing #root element");
}

mountHalftoneBackground();

ReactDOM.createRoot(rootEl).render(
	<React.StrictMode>
		<OutsideNow />
	</React.StrictMode>,
);
