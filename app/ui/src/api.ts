// Types mirror the Rust structs (serde) and thin wrappers around invoke().
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Lang = "c" | "cpp";
export type ProblemKind = "traditional" | "answer" | "interactive" | "communication";
export type Subfolder = "auto" | "inside" | "outside";
export type Comparator = "lines" | "strict" | "tokens" | "real" | "binary";
export type CheckerKind = "testlib" | "lemon";
export type Verdict = "AC" | "PC" | "WA" | "TLE" | "MLE" | "OLE" | "RE" | "NoOutput" | "Skipped" | "SystemError";
export type Status = "Judged" | "NoSource" | "CompileError" | "SystemError";

export interface CompilerConfig {
  lang: Lang;
  path: string;
  args: string;
}

export interface CompilerInfo extends CompilerConfig {
  version: string;
}

export interface AppSettings {
  compilers: CompilerConfig[];
  workers: number;
  pin_cores: boolean;
  patch_stack: boolean;
  output_limit_mb: number;
  compile_time_ms: number;
  compile_memory_mb: number;
  checker_time_ms: number;
  checker_memory_mb: number;
  language: string;
  recent: string[];
}

export interface Case {
  input: string;
  output: string;
}

export interface Subtask {
  score: number;
  time_ms: number;
  memory_mb: number;
  cases: Case[];
  depends?: number[];
}

export interface Problem {
  name: string;
  title?: string;
  kind: ProblemKind;
  source_name?: string;
  subfolder: Subfolder;
  stdio: boolean;
  input_file?: string;
  output_file?: string;
  comparator: Comparator;
  real_eps: number;
  checker?: { path: string; kind: CheckerKind } | null;
  answer_ext?: string;
  grader?: { header_path: string; header_name: string; grader_path: string } | null;
  interactor?: string | null;
  communication?: { sources: string[]; interfaces: string[] } | null;
  compile_args?: string;
  subtask: Subtask[];
}

export interface Contest {
  title: string;
  problem: Problem[];
}

export interface CellView {
  contestant: string;
  problem: string;
  score: number;
  full: number;
  label: string;
  status: Status;
  time_ms: number;
  stale: boolean;
}

export interface ContestView {
  root: string;
  contest: Contest;
  contestants: string[];
  cells: CellView[];
}

export interface CaseResult {
  input: string;
  verdict: Verdict;
  ratio: number;
  time_ms: number;
  memory_kb: number;
  message?: string;
}

export interface SubtaskResult {
  score: number;
  full: number;
  cases: CaseResult[];
}

export interface ProblemResult {
  contestant: string;
  problem: string;
  status: Status;
  score: number;
  full: number;
  time_ms: number;
  message?: string;
  subtasks: SubtaskResult[];
  judged_at: number;
}

export interface WorkerView {
  contestant: string;
  problem: string;
  cases_done: number;
}

export interface ProgressView {
  running: boolean;
  total: number;
  done: number;
  workers: (WorkerView | null)[];
  counts: Record<string, number>;
  elapsed_ms: number;
  updates: CellView[];
  messages: string[];
  error: string | null;
}

export interface AppInfo {
  version: string;
  settings_path: string;
  cpu_cores: number;
  default_workers: number;
}

export interface ProbeView {
  name: string;
  passed: boolean;
  detail: string;
}

export interface SourceFile {
  path: string;
  text: string;
}

export interface JudgeRequest {
  problems?: string[];
  contestants?: string[];
  only_stale?: boolean;
  pairs?: [string, string][];
}

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  getSettings: () => invoke<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) => invoke<void>("save_settings", { settings }),
  detectCompilers: () => invoke<CompilerInfo[]>("detect_compilers"),
  openContest: (path: string) => invoke<ContestView>("open_contest", { path }),
  reloadContest: () => invoke<ContestView>("reload_contest"),
  closeContest: () => invoke<void>("close_contest"),
  saveContestConfig: (contest: Contest) => invoke<ContestView>("save_contest_config", { contest }),
  detectProblems: (timeMs: number, memoryMb: number) =>
    invoke<Problem[]>("detect_problems", { timeMs, memoryMb }),
  getResult: (contestant: string, problem: string) =>
    invoke<ProblemResult | null>("get_result", { contestant, problem }),
  listDataFiles: (sub: string) => invoke<string[]>("list_data_files", { sub }),
  readSource: (contestant: string, problem: string) =>
    invoke<SourceFile[]>("read_source", { contestant, problem }),
  startJudge: (req: JudgeRequest) => invoke<void>("start_judge", { req }),
  cancelJudge: () => invoke<void>("cancel_judge"),
  judgeStatus: () => invoke<ProgressView | null>("judge_status"),
  runSelftest: () => invoke<ProbeView[]>("run_selftest"),
  onProgress: (f: (p: ProgressView) => void): Promise<UnlistenFn> =>
    listen<ProgressView>("judge-progress", (e) => f(e.payload)),
};

export function fmtScore(x: number): string {
  if (Math.abs(x - Math.round(x)) < 1e-9) return String(Math.round(x));
  return x.toFixed(2).replace(/0+$/, "").replace(/\.$/, "");
}

export function fullScore(p: Problem): number {
  return p.subtask.reduce((s, t) => s + t.score, 0);
}

export function caseCount(p: Problem): number {
  return p.subtask.reduce((s, t) => s + t.cases.length, 0);
}
