// Development only: fake backend so the UI can be previewed in a plain
// browser (`npm run dev`). Never bundled into the desktop app.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { emit } from "@tauri-apps/api/event";
import type { AppSettings, CellView, Contest, ProblemResult, ProgressView, Subtask } from "./api";

function sub(prefix: string, n: number, score: number, deps = false): Subtask[] {
  return Array.from({ length: n }, (_, i) => ({
    score,
    time_ms: 1000,
    memory_mb: 512,
    cases: [{ input: `${prefix}/${prefix}${i + 1}.in`, output: `${prefix}/${prefix}${i + 1}.out` }],
    depends: deps && i > 0 ? [i] : undefined,
  }));
}

const contest: Contest = {
  title: "NOIP 2026 模拟赛（第三场）",
  problem: [
    { name: "sum", title: "A + B", kind: "traditional", subfolder: "auto", stdio: true, comparator: "lines", real_eps: 1e-6, subtask: sub("sum", 10, 10) },
    { name: "path", title: "最短路", kind: "traditional", subfolder: "auto", stdio: false, comparator: "lines", real_eps: 1e-6, subtask: sub("path", 20, 5, true) },
    { name: "geo", title: "几何", kind: "traditional", subfolder: "auto", stdio: true, comparator: "real", real_eps: 1e-6, checker: { path: "geo/checker.cpp", kind: "testlib" }, subtask: sub("geo", 4, 25) },
    { name: "guess", title: "猜数", kind: "interactive", subfolder: "auto", stdio: true, comparator: "lines", real_eps: 1e-6, interactor: "guess/interactor.cpp", subtask: sub("guess", 5, 20) },
  ],
};

let seed = 7;
const rnd = () => ((seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648);
const names = Array.from({ length: 120 }, (_, i) => `student${String(i + 1).padStart(3, "0")}`);
const labels = ["WA", "TLE", "RE", "MLE"];

function makeCell(c: string, p: (typeof contest.problem)[number]): CellView {
  const full = p.subtask.reduce((s, t) => s + t.score, 0);
  const r = rnd();
  if (r < 0.06) return { contestant: c, problem: p.name, score: 0, full, label: "Missing", status: "NoSource", time_ms: 0, judged_at: 1794000000, stale: false };
  if (r < 0.1) return { contestant: c, problem: p.name, score: 0, full, label: "CE", status: "CompileError", time_ms: 0, judged_at: 1794000000, stale: false };
  const score = r > 0.55 ? full : Math.round(full * rnd() * 10) / 10 > full * 0.1 ? Math.round(full * rnd()) : 0;
  return {
    contestant: c, problem: p.name, score, full, label: score >= full ? "AC" : labels[Math.floor(rnd() * labels.length)],
    status: "Judged", time_ms: Math.round(rnd() * 3000), judged_at: 1794000000 + Math.round(rnd() * 3600), stale: rnd() < 0.03,
  };
}

const cells: CellView[] = [];
for (const c of names) for (const p of contest.problem) if (rnd() > 0.02) cells.push(makeCell(c, p));

const settings: AppSettings = {
  compilers: [
    { lang: "cpp", path: "C:\\mingw64\\bin\\g++.exe", args: "-O2 -std=c++17 -static -DONLINE_JUDGE" },
    { lang: "c", path: "C:\\mingw64\\bin\\gcc.exe", args: "-O2 -std=c11 -static -DONLINE_JUDGE -lm" },
  ],
  workers: 0, pin_cores: true, overlap_compile: true, cache_limit_mb: 10240, patch_stack: true, recheck_tle: true, output_limit_mb: 256, compile_time_ms: 20000, compile_memory_mb: 2048,
  checker_time_ms: 10000, checker_memory_mb: 2048, language: "zh-CN", recent: ["D:\\contests\\noip-mock-3", "D:\\contests\\csp-s-2026"],
};

function result(c: string, pname: string): ProblemResult | null {
  const cell = cells.find((x) => x.contestant === c && x.problem === pname);
  const p = contest.problem.find((x) => x.name === pname)!;
  if (!cell) return null;
  if (cell.status === "CompileError")
    return { contestant: c, problem: pname, status: "CompileError", score: 0, full: cell.full, time_ms: 0, judged_at: Date.now() / 1000, subtasks: [],
      message: `${pname}.cpp: In function 'int main()':\n${pname}.cpp:7:5: error: 'vectr' was not declared in this scope\n    7 |     vectr<int> a(n);\n      |     ^~~~~` };
  let ok = Math.round((cell.score / cell.full) * p.subtask.length);
  return {
    contestant: c, problem: pname, status: cell.status, score: cell.score, full: cell.full, time_ms: cell.time_ms, judged_at: Date.now() / 1000,
    subtasks: p.subtask.map((s) => {
      const pass = ok-- > 0;
      return { score: pass ? s.score : 0, full: s.score, cases: s.cases.map((cs) => ({
        input: cs.input, verdict: pass ? "AC" : (cell.label as never), ratio: pass ? 1 : 0, time_ms: Math.round(rnd() * 900), memory_kb: 1200 + Math.round(rnd() * 50000),
        message: pass ? "" : cell.label === "WA" ? "Line 3: expected `42`, found `41`" : cell.label === "RE" ? "Access violation (invalid memory access) (0xC0000005)" : "",
      })) };
    }),
  };
}

let open = false;
let onlineOn = false;
function onlineView() {
  return onlineOn
    ? { running: true, addr: "0.0.0.0:8080", urls: ["http://192.168.1.20:8080/"], local_url: "http://127.0.0.1:8080/", bind: "0.0.0.0:8080", judging: true, judging_error: null, admin_password: "k7m2pq9x", queued: 2, running_jobs: 4, online: 87, format: "icpc" }
    : { running: false, addr: "", urls: [], local_url: "", bind: "0.0.0.0:8080", judging: false, judging_error: null, admin_password: null, queued: 0, running_jobs: 0, online: 0, format: "" };
}
async function simulate(total: number) {
  const view: ProgressView = { running: true, compile_done: 0, compile_total: total, total, done: 0, workers: [null, null, null, null], counts: {}, elapsed_ms: 0, updates: [], messages: [], log: [], error: null };
  const start = Date.now();
  for (let i = 0; i < total; i++) {
    await new Promise((r) => setTimeout(r, 60));
    const c = names[Math.floor(i / contest.problem.length) % names.length];
    const p = contest.problem[i % contest.problem.length];
    const cell = makeCell(c, p);
    const idx = cells.findIndex((x) => x.contestant === c && x.problem === p.name);
    if (idx >= 0) cells[idx] = cell; else cells.push(cell);
    view.done++;
    // Compiling runs ahead of judging and pauses for a while (cache limit).
    view.compile_done = Math.min(total, view.done + 3);
    view.compile_paused = view.done > total * 0.3 && view.done < total * 0.5;
    view.counts[cell.label] = (view.counts[cell.label] ?? 0) + 1;
    view.workers = view.workers.map((_, k) => ({ contestant: names[(i + k) % names.length], problem: contest.problem[(i + k) % 4].name, cases_done: k * 3 }));
    view.elapsed_ms = Date.now() - start;
    view.updates = [cell];
    const pr = contest.problem.find((x) => x.name === cell.problem)!;
    view.log = [
      ...pr.subtask.slice(0, 3).map((_, k) => ({ contestant: c, problem: pr.name, subtask: k + 1, input: "", verdict: k === 2 && cell.label !== "AC" ? cell.label : "AC", time_ms: Math.round(rnd() * 300), memory_kb: 2048 + Math.round(rnd() * 9000), score: null, message: k === 2 && cell.label === "WA" ? "Line 1: expected `42`, found `41`" : "" })),
      { contestant: c, problem: pr.name, subtask: 0, input: "", verdict: cell.label, time_ms: cell.time_ms, memory_kb: 0, score: [cell.score, cell.full] as [number, number], message: "" },
    ];
    await emit("judge-progress", { ...view });
  }
  await emit("judge-progress", { ...view, running: false, workers: [], updates: [], log: [] });
}

function summary(xs: number[], full: number) {
  const s = [...xs].sort((a, b) => a - b);
  const q = (p: number) => s[Math.round(p * (s.length - 1))] ?? 0;
  const mean = xs.reduce((a, b) => a + b, 0) / Math.max(1, xs.length);
  const std = Math.sqrt(xs.reduce((a, b) => a + (b - mean) ** 2, 0) / Math.max(1, xs.length));
  return {
    count: xs.length, submitted: xs.length - 3, full, mean, std_dev: std, min: s[0] ?? 0, q1: q(0.25), median: q(0.5), q3: q(0.75),
    max: s[s.length - 1] ?? 0, full_count: xs.filter((x) => x >= full).length, zero_count: xs.filter((x) => x === 0).length,
    discrimination: 0.42 + rnd() * 0.3,
  };
}

function mockStats() {
  const score = (c: string, p: string) => cells.find((x) => x.contestant === c && x.problem === p)?.score ?? 0;
  const per = contest.problem.map((p) => names.map((c) => score(c, p.name)));
  const totals = names.map((_, i) => per.reduce((s, row) => s + row[i], 0));
  const full = 400;
  const hist = Array.from({ length: 20 }, (_, i) => {
    const lo = i * 20, hi = lo + 20;
    const count = totals.filter((x) => x >= lo && (x < hi || (i === 19 && x <= hi))).length;
    return { lo, hi, count, ratio: count / names.length, prefix: 0, suffix: totals.filter((x) => x >= lo).length };
  });
  return {
    contestants: names.length, overall: summary(totals, full), counts: [], histogram: hist,
    problems: contest.problem.map((p, i) => ({
      name: p.name, title: p.title, summary: summary(per[i], 100), counts: [], histogram: [],
      labels: { AC: 60 + i * 3, WA: 20, TLE: 12 - i, RE: 6, CE: 4, Missing: 7 }, verdicts: {},
      subtasks: p.subtask.map((_, k) => ({ full: 10, mean: 5, pass_rate: Math.max(0, 0.95 - k * 0.04 - i * 0.08 - rnd() * 0.1), case_pass_rates: [] })),
      mean_time_ms: 120 + i * 80,
    })),
    correlation: contest.problem.map((_, i) => contest.problem.map((_, j) => (i === j ? 1 : Math.round((0.6 - Math.abs(i - j) * 0.25) * 100) / 100))),
    totals, problem_scores: per,
    groups: ["一中", "二中", "实验中学", "外国语"].map((v, k) => ({ key: "学校", value: v, summary: summary(totals.filter((_, i) => i % 4 === k), full), problem_means: [80 - k * 5, 70 - k * 4, 60 - k * 6, 55 - k * 3] })),
    group_keys: ["学校"],
  };
}

export function installMocks() {
  mockWindows("main");
  mockIPC(
    async (cmd, args) => {
      const a = (args ?? {}) as Record<string, unknown>;
      switch (cmd) {
        case "app_info": return { version: "0.1.0-dev", settings_path: "C:\\Users\\you\\AppData\\Roaming\\OIJudger\\settings.toml", cpu_cores: 6, default_workers: 4 };
        case "get_settings": return settings;
        case "save_settings": Object.assign(settings, a.settings); return null;
        case "detect_compilers": return settings.compilers.map((c) => ({ ...c, version: "g++.exe (MinGW-Builds) 16.1.0" }));
        case "open_contest": case "reload_contest": open = true; return { root: "D:\\contests\\noip-mock-3", contest, contestants: names, cells };
        case "save_contest_config": Object.assign(contest, a.contest); return { root: "D:\\contests\\noip-mock-3", contest, contestants: names, cells };
        case "close_contest": open = false; return null;
        case "online_status": return onlineView();
        case "online_start": onlineOn = true; return onlineView();
        case "online_stop": onlineOn = false; return null;
        case "online_open_jury": return null;
        case "cache_usage": return { files: 960, bytes: 119_000_000 };
        case "clear_cache": return { files: 960, bytes: 119_000_000 };
        case "get_result": return result(a.contestant as string, a.problem as string);
        case "read_source": return [{ path: `D:\\contests\\noip-mock-3\\source\\${a.contestant}\\${a.problem}\\${a.problem}.cpp`, text: "#include <bits/stdc++.h>\nusing namespace std;\n\nint main() {\n    long long a, b;\n    cin >> a >> b;\n    cout << a + b << endl;\n}\n" }];
        case "list_data_files": return Array.from({ length: 10 }, (_, i) => [`${a.sub}${i + 1}.in`, `${a.sub}${i + 1}.out`]).flat();
        case "detect_problems": return [];
        case "judge_status": return null;
        case "start_judge": simulate(40); return null;
        case "cancel_judge": return null;
        case "get_stats": return mockStats();
        case "export_results": return null;
        case "similarity":
          return [
            { a: "student014", b: "student077", similarity: 0.96, jaccard: 0.91, shared: 54, a_lines: [[4, 7]], b_lines: [[3, 7]] },
            { a: "student031", b: "student090", similarity: 0.82, jaccard: 0.64, shared: 31, a_lines: [[5, 6]], b_lines: [[4, 7]] },
            { a: "student002", b: "student118", similarity: 0.71, jaccard: 0.5, shared: 19, a_lines: [[6, 7]], b_lines: [[6, 7]] },
          ].filter((p) => p.similarity >= (a.threshold as number));
        case "run_selftest":
          await new Promise((r) => setTimeout(r, 800));
          return ["normal program runs", "network is blocked (loopback, internet, DNS)", "files outside the sandbox cannot be read", "busy loop is stopped (TLE)"]
            .map((name) => ({ name, passed: true, detail: "BLOCKED" }));
        default:
          if (cmd.startsWith("plugin:")) return null;
          console.warn("unmocked command", cmd, a);
          return null;
      }
    },
    { shouldMockEvents: true },
  );
  void open;
}
