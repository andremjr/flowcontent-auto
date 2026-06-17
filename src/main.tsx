import { createRoot } from "react-dom/client";
import App from "./App";
import { installDiagnostics } from "./lib/diagnostics";
import "./styles.css";

installDiagnostics();

createRoot(document.getElementById("root")!).render(
  <App />,
);
