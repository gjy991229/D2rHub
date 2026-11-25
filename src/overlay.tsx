import React from "react";
import ReactDOM from "react-dom/client";
import { Overlay } from "./pages/Overlay";
import { I18nRuntime } from "./i18n";
import "./styles/globals.css";
import "./styles/workspace.css";
import { syncMotionPreference } from "./store/motionPreference";

syncMotionPreference();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <I18nRuntime />
    <Overlay />
  </React.StrictMode>
);
