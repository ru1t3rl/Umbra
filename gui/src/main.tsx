import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

const container = document.getElementById("root");
if (!container) throw new Error("missing #root element");

// Placeholder: providers, router and the shells arrive in M5b WI-7.
createRoot(container).render(<StrictMode />);
