/// <reference types="vite/client" />

import "solid-js";

declare module "solid-js" {
  namespace JSX {
    interface Directives {
      droppable: true;
      draggable: true;
      centerDroppable: true;
    }
  }
}
