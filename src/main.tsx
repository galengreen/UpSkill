import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource-variable/geist";
import "@fontsource-variable/geist-mono";
import "./styles.css";
import { App } from "./App";

const ua = navigator.userAgent;
document.documentElement.dataset.platform = ua.includes("Mac") ? "mac" : ua.includes("Linux") ? "linux" : "other";

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
