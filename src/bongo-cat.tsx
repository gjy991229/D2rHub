import React from "react";
import ReactDOM from "react-dom/client";
import { BongoCatWindow } from "./pages/BongoCatWindow";
import { I18nRuntime } from "./i18n";
import "./styles/globals.css";
import "./styles/workspace.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <I18nRuntime />
    <BongoCatWindow />
  </React.StrictMode>
);
