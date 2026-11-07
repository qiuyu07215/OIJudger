// Typed client for the OIJudger server API (/api/...).

export type Role = "contestant" | "jury" | "balloon" | "admin";
export type Format = "icpc" | "oi" | "ioi";
export type Phase = "not_scheduled" | "before" | "running" | "ended";
export type Outcome = "pending" | "accepted" | "rejected" | "compile_error" | "error";
export type JudgingState = "queued" | "judging" | "done" | "cancelled";

export interface Me {
  id: number;
  username: string;
  display_name: string;
  group_name: string;
  seat: string;
  role: Role;
}

export interface ProblemInfo {
  name: string;
  letter: string;
  title: string;
  color: string;
  full: number;
  time_ms: number;
  memory_mb: number;
  stdio: boolean;
  input_file: string;
  output_file: string;
  kind: string;
  statement: "" | "pdf" | "md" | "html";
  samples: string[];
}

export interface Info {
  title: string;
  format: Format;
  phase: Phase;
  now_ms: number;
  start_ms: number | null;
  end_ms: number | null;
  freeze_at: number | null;
  frozen: boolean;
  public_scoreboard: boolean;
  results_published: boolean;
  oi_feedback: "none" | "compile";
  max_source_kb: number;
  submit_interval_s: number;
  freeze_min: number;
  medals: [number, number, number];
  problems: ProblemInfo[];
  me: Me | null;
}

export interface Cell {
  attempts: number;
  pending: number;
  solved: boolean;
  time_min: number;
  first: boolean;
  score: number | null;
}

export interface BoardRow {
  username: string;
  display_name: string;
  group_name: string;
  account: number;
  rank: number;
  solved: number;
  penalty: number;
  last_ac_ms: number;
  total: number;
  cells: Cell[];
}

export interface BoardProblem {
  name: string;
  letter: string;
  title: string;
  color: string;
  full: number;
}

export interface Scoreboard {
  format: Format;
  problems: BoardProblem[];
  rows: BoardRow[];
  frozen: boolean;
  hidden: boolean;
}

export interface Submission {
  id: number;
  account_id: number;
  username: string;
  display_name: string;
  problem: string;
  lang: string;
  at_ms: number;
  contest_ms: number;
  state: JudgingState;
  outcome: Outcome;
  label: string;
  score: number | null;
  full: number;
  time_ms: number;
  ignored: boolean;
}

export interface CaseView {
  verdict: string;
  time_ms: number;
  memory_kb: number;
  message: string;
  input: string;
}

export interface SubmissionDetail extends Submission {
  source: string;
  message: string;
  subtasks: { score: number; full: number; cases: CaseView[] }[];
}

export interface Clarification {
  id: number;
  from_account: number | null;
  to_account: number | null;
  problem: string;
  question: string;
  answer: string;
  public: boolean;
  at_ms: number;
  answered_ms: number | null;
  from_name: string;
  to_name: string;
}

export interface Announcement {
  id: number;
  title: string;
  body: string;
  at_ms: number;
}

export interface Balloon {
  id: number;
  account_id: number;
  problem: string;
  submission_id: number;
  at_ms: number;
  first: boolean;
  done: boolean;
  username: string;
  display_name: string;
  seat: string;
  letter: string;
  color: string;
  has: string[];
}

export interface PrintJob {
  id: number;
  account_id: number;
  filename: string;
  content: string;
  at_ms: number;
  done: boolean;
  username?: string;
  display_name?: string;
  seat?: string;
}

export interface Account {
  id: number;
  username: string;
  display_name: string;
  group_name: string;
  seat: string;
  role: Role;
  ip_lock: string;
  disabled: boolean;
}

export interface Credential {
  username: string;
  display_name: string;
  seat: string;
  password: string;
}

export interface OnlineConfig {
  format: Format;
  start: string;
  duration_min: number;
  freeze_min: number;
  penalty_min: number;
  compile_error_penalty: boolean;
  ioi_scoring: "max_subtask" | "max_submission";
  oi_feedback: "none" | "compile";
  max_source_kb: number;
  submit_interval_s: number;
  bind: string;
  public_scoreboard: boolean;
  medals: [number, number, number];
}

export interface JuryStatus {
  judging_active: boolean;
  queued: number;
  running: number;
  online: number;
  accounts: number;
  submissions: number;
  unanswered: number;
}

export interface ResolveStep {
  account: number;
  problem: number;
  cell: Cell;
  solved: number;
  penalty: number;
  last_ac_ms: number;
}

export interface OfflineJudging {
  running: boolean;
  done: number;
  total: number;
  error: string | null;
}

export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
    public detail?: unknown,
  ) {
    super(message);
  }
}

async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
  const res = await fetch(path, {
    method,
    headers: { "X-Requested-With": "oij", ...(body !== undefined ? { "Content-Type": "application/json" } : {}) },
    body: body !== undefined ? JSON.stringify(body) : undefined,
    credentials: "same-origin",
  });
  if (res.status === 204 || res.status === 202) return undefined as T;
  const text = await res.text();
  const data = text ? JSON.parse(text) : undefined;
  if (!res.ok) {
    throw new ApiError(res.status, data?.error ?? "error", data?.message ?? res.statusText, data?.detail);
  }
  return data as T;
}

const get = <T>(p: string) => call<T>("GET", p);
const post = <T>(p: string, b: unknown = {}) => call<T>("POST", p, b);

export const api = {
  info: () => get<Info>("/api/info"),
  login: (username: string, password: string) => post<Me>("/api/login", { username, password }),
  logout: () => post<void>("/api/logout"),
  scoreboard: () => get<Scoreboard>("/api/scoreboard"),
  submit: (problem: string, lang: string, source: string) => post<{ id: number }>("/api/submit", { problem, lang, source }),
  submissions: () => get<Submission[]>("/api/submissions"),
  submission: (id: number) => get<SubmissionDetail>(`/api/submissions/${id}`),
  clarifications: () => get<Clarification[]>("/api/clarifications"),
  ask: (problem: string, question: string) => post<{ id: number }>("/api/clarifications", { problem, question }),
  announcements: () => get<Announcement[]>("/api/announcements"),
  print: (filename: string, content: string) => post<{ id: number }>("/api/print", { filename, content }),
  myPrints: () => get<PrintJob[]>("/api/print"),
  printQueue: () => get<PrintJob[]>("/api/print/queue"),
  printDone: (id: number, done: boolean) => post<void>(`/api/print/queue/${id}`, { done }),
  balloons: () => get<Balloon[]>("/api/balloons"),
  balloonDone: (id: number, done: boolean) => post<void>(`/api/balloons/${id}`, { done }),
  // jury
  status: () => get<JuryStatus>("/api/jury/status"),
  rejudge: (ids: number[]) => post<{ count: number }>("/api/jury/rejudge", { ids }),
  rejudgeAll: (problem: string, team: string, includeCorrect: boolean) =>
    post<{ count: number }>("/api/jury/rejudge", { problem, team, include_correct: includeCorrect }),
  ignore: (id: number, ignored: boolean) => post<void>(`/api/jury/submissions/${id}/ignore`, { ignored }),
  teamRow: () => get<Scoreboard | null>("/api/team-row"),
  problemsetStats: () => get<{ problem: string; intervals: [number, number][] }[]>("/api/problemset-stats"),
  answer: (id: number, answer: string, pub: boolean) => post<void>(`/api/jury/clarifications/${id}/answer`, { answer, public: pub }),
  message: (to: string, problem: string, text: string) => post<void>("/api/jury/messages", { to, problem, text }),
  announce: (title: string, body: string) => post<void>("/api/jury/announcements", { title, body }),
  deleteAnnouncement: (id: number) => call<void>("DELETE", `/api/jury/announcements/${id}`),
  freeze: (unfrozen: boolean) => post<void>("/api/jury/freeze", { unfrozen }),
  publish: (published: boolean) => post<void>("/api/jury/publish", { published }),
  reload: () => post<void>("/api/jury/reload"),
  resolver: () => get<{ board: Scoreboard; steps: ResolveStep[] }>("/api/jury/resolver"),
  offlineStatus: () => get<OfflineJudging | null>("/api/jury/judge-offline"),
  offlineStart: () => post<void>("/api/jury/judge-offline"),
  config: () => get<OnlineConfig>("/api/jury/config"),
  saveConfig: (c: OnlineConfig) => call<void>("PUT", "/api/jury/config", c),
  accounts: () => get<Account[]>("/api/jury/accounts"),
  createAccount: (a: Partial<Account> & { password?: string }) => post<Credential>("/api/jury/accounts", a),
  importAccounts: (csv: string) => post<Credential[]>("/api/jury/accounts/import", { csv }),
  updateAccount: (a: Account) => call<void>("PUT", `/api/jury/accounts/${a.id}`, a),
  deleteAccount: (id: number) => call<void>("DELETE", `/api/jury/accounts/${id}`),
  resetPassword: (id: number) => post<{ password: string }>(`/api/jury/accounts/${id}/reset`, {}),
};

export type Notice =
  | { type: "submission"; id: number; account: number }
  | { type: "scoreboard" }
  | { type: "clarification"; id: number; from: number | null; to: number | null; public: boolean }
  | { type: "announcement"; id: number }
  | { type: "balloon" }
  | { type: "print" }
  | { type: "contest" }
  | { type: "queue"; queued: number; running: number };

/** Server-sent events; reconnects automatically (EventSource does). */
export function listen(onNotice: (n: Notice) => void, onReconnect: () => void): () => void {
  let es: EventSource | null = null;
  let closed = false;
  const open = () => {
    es = new EventSource("/api/events");
    es.onmessage = (e) => {
      try {
        onNotice(JSON.parse(e.data));
      } catch {
        /* ignore */
      }
    };
    // After a reconnect, anything may have changed.
    es.addEventListener("hello", () => onReconnect());
  };
  open();
  return () => {
    closed = true;
    es?.close();
    void closed;
  };
}
