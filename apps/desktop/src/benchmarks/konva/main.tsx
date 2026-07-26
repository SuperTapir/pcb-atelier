import { createRoot } from "react-dom/client";

import "@/index.css";

import { KonvaBenchmark } from "./KonvaBenchmark";

createRoot(document.getElementById("benchmark-root")!).render(
  <KonvaBenchmark />,
);
