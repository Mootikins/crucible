import { render } from "solid-js/web";
import { ReparentingSpike } from "@/lib/solid-layout/ReparentingSpike";

const root = document.getElementById("root");
if (root) {
  render(() => <ReparentingSpike />, root);
}
