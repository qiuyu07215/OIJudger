// Global application state.
import { computed, reactive, shallowRef, triggerRef } from "vue";
import {
  api,
  type AppInfo,
  type AppSettings,
  type CellView,
  type Contest,
  type ContestView,
  type LogLine,
  type ProgressView,
} from "./api";

const key = (c: string, p: string) => `${c}\u0000${p}`;

export const state = reactive({
  info: null as AppInfo | null,
  settings: null as AppSettings | null,
  root: "" as string,
  contest: null as Contest | null,
  contestants: [] as string[],
  progress: null as ProgressView | null,
  busy: false,
  /** The judging window is open (LemonLime-style dialog). */
  showJudging: false,
});

/** Rolling judging log (most recent last). */
export const log = shallowRef<LogLine[]>([]);
const LOG_MAX = 3000;

export function appendLog(lines: LogLine[]) {
  if (!lines.length) return;
  const next = log.value.concat(lines);
  log.value = next.length > LOG_MAX ? next.slice(next.length - LOG_MAX) : next;
}

export function clearLog() {
  log.value = [];
}

/** (contestant, problem) → cell. Kept in a shallowRef: thousands of entries. */
export const cells = shallowRef(new Map<string, CellView>());

export function getCell(contestant: string, problem: string): CellView | undefined {
  return cells.value.get(key(contestant, problem));
}

function applyView(v: ContestView) {
  state.root = v.root;
  state.contest = v.contest;
  state.contestants = v.contestants;
  const m = new Map<string, CellView>();
  for (const c of v.cells) m.set(key(c.contestant, c.problem), c);
  cells.value = m;
}

export function applyUpdates(updates: CellView[]) {
  if (!updates.length) return;
  const m = cells.value;
  for (const c of updates) m.set(key(c.contestant, c.problem), c);
  triggerRef(cells);
}

export const judging = computed(() => !!state.progress?.running);

export interface Row {
  rank: number;
  contestant: string;
  total: number;
  time: number;
  stale: number;
}

/** Ranking rows (ties share a rank). */
export const rows = computed<Row[]>(() => {
  const c = state.contest;
  if (!c) return [];
  const m = cells.value;
  const out: Row[] = state.contestants.map((name) => {
    let total = 0;
    let time = 0;
    let stale = 0;
    for (const p of c.problem) {
      const cell = m.get(key(name, p.name));
      if (!cell) {
        stale++;
        continue;
      }
      total += cell.score;
      time += cell.time_ms;
      if (cell.stale) stale++;
    }
    return { rank: 0, contestant: name, total, time, stale };
  });
  out.sort((a, b) => b.total - a.total || a.contestant.localeCompare(b.contestant, undefined, { numeric: true }));
  out.forEach((r, i) => {
    r.rank = i > 0 && Math.abs(r.total - out[i - 1].total) < 1e-9 ? out[i - 1].rank : i + 1;
  });
  return out;
});

export const staleCount = computed(() => rows.value.reduce((s, r) => s + r.stale, 0));

export async function openContest(path: string) {
  state.busy = true;
  try {
    applyView(await api.openContest(path));
    state.settings = await api.getSettings();
  } finally {
    state.busy = false;
  }
}

export async function reloadContest() {
  applyView(await api.reloadContest());
}

export async function saveContest(contest: Contest) {
  applyView(await api.saveContestConfig(contest));
}

export async function closeContest() {
  await api.closeContest();
  state.contest = null;
  state.root = "";
  state.contestants = [];
  cells.value = new Map();
}
