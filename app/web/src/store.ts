import { reactive, ref } from "vue";
import { api, listen, type Info, type Me, type Notice } from "./api";

export const state = reactive({
  info: null as Info | null,
  me: null as Me | null,
  /** server clock minus local clock, ms */
  offset: 0,
  loaded: false,
  connected: false,
});

/** Ticks every second (countdowns). */
export const now = ref(Date.now());
setInterval(() => (now.value = Date.now() + state.offset), 1000);

export function serverNow() {
  return Date.now() + state.offset;
}

export async function refreshInfo() {
  const t0 = Date.now();
  const info = await api.info();
  const t1 = Date.now();
  state.offset = info.now_ms - (t0 + t1) / 2;
  now.value = Date.now() + state.offset;
  state.info = info;
  state.me = info.me;
  state.loaded = true;
}

type Handler = (n: Notice) => void;
const handlers = new Set<Handler>();

/** Subscribe to server notices; returns an unsubscribe function. */
export function onNotice(h: Handler): () => void {
  handlers.add(h);
  return () => handlers.delete(h);
}

let stop: (() => void) | null = null;

/** (Re)connect the event stream, e.g. after logging in. */
export function connect() {
  stop?.();
  stop = listen(
    (n) => {
      if (n.type === "contest") void refreshInfo();
      handlers.forEach((h) => h(n));
    },
    () => {
      state.connected = true;
      void refreshInfo();
      handlers.forEach((h) => h({ type: "scoreboard" }));
    },
  );
}

/** Call `fn` at most once per `ms`, trailing (for refetch storms). */
export function throttle(fn: () => void, ms = 800): () => void {
  let t: ReturnType<typeof setTimeout> | null = null;
  let last = 0;
  return () => {
    if (t) return;
    const wait = Math.max(0, last + ms - Date.now());
    t = setTimeout(() => {
      t = null;
      last = Date.now();
      fn();
    }, wait);
  };
}

export const isStaff = () => !!state.me && state.me.role !== "contestant";
export const isJury = () => !!state.me && (state.me.role === "jury" || state.me.role === "admin");
export const isAdmin = () => state.me?.role === "admin";

/** Global UI state: the submit dialog. */
export const ui = reactive({ submitOpen: false, submitProblem: null as string | null });
export function openSubmit(problem: string | null = null) {
  ui.submitProblem = problem;
  ui.submitOpen = true;
}

// ---- results not looked at yet (bold in lists, as DOMjudge) ----
const seenKey = () => `oij.seen.${state.me?.id ?? 0}`;
export const seen = reactive(new Set<number>());
export function loadSeen() {
  seen.clear();
  try {
    for (const id of JSON.parse(localStorage.getItem(seenKey()) || "[]")) seen.add(id);
  } catch {
    /* storage unavailable */
  }
}
export function markSeen(id: number) {
  if (seen.has(id)) return;
  seen.add(id);
  try {
    localStorage.setItem(seenKey(), JSON.stringify([...seen].slice(-2000)));
  } catch {
    /* ignore */
  }
}

// ---- browser notifications ("Enable Notifications") ----
export const notifyState = reactive({ permission: typeof Notification === "undefined" ? "denied" : Notification.permission });
export async function enableNotifications() {
  if (typeof Notification === "undefined") return;
  notifyState.permission = await Notification.requestPermission();
}
export function browserNotify(title: string, body: string) {
  if (typeof Notification === "undefined" || Notification.permission !== "granted" || document.hasFocus()) return;
  try {
    new Notification(title, { body, tag: `${title}:${body}`.slice(0, 64) });
  } catch {
    /* ignore */
  }
}
