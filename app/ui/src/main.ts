import { createApp } from "vue";
import App from "./App.vue";
import { i18n } from "./i18n";
import "./style.css";

async function boot() {
  // Plain browser during development: use a fake backend.
  if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
    (await import("./mock")).installMocks();
  }
  createApp(App).use(i18n).mount("#app");
}
boot();
