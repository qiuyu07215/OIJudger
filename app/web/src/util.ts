import type { Outcome, Submission } from "./api";

export const pad = (n: number) => String(n).padStart(2, "0");

/** h:mm:ss for a duration in ms (negative → 0). */
export function hms(ms: number) {
  const s = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(s / 3600)}:${pad(Math.floor(s / 60) % 60)}:${pad(s % 60)}`;
}

/** Contest time (minutes) as h:mm. */
export function contestTime(ms: number) {
  const m = Math.floor(ms / 60000);
  return `${Math.floor(m / 60)}:${pad(m % 60)}`;
}

export function clock(ms: number) {
  const d = new Date(ms);
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

export function dateTime(ms: number) {
  const d = new Date(ms);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function fmtScore(x: number | null | undefined) {
  if (x === null || x === undefined) return "";
  return Number.isInteger(x) ? String(x) : x.toFixed(2).replace(/0+$/, "").replace(/\.$/, "");
}

/** Colour class for a verdict: "ok", "bad", "ce", "pending", "info". */
export function verdictClass(s: Pick<Submission, "state" | "outcome" | "label">) {
  if (s.state !== "done") return "pending";
  const o: Outcome = s.outcome;
  if (o === "accepted") return "ok";
  if (o === "compile_error") return "ce";
  if (o === "error") return "se";
  if (o === "rejected") return "bad";
  return "info";
}

/** Readable text colour on a balloon colour. */
export function contrastText(hex: string) {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return "inherit";
  const n = parseInt(m[1], 16);
  const r = n >> 16, g = (n >> 8) & 255, b = n & 255;
  return 0.299 * r + 0.587 * g + 0.114 * b > 150 ? "#141413" : "#ffffff";
}

export function download(name: string, text: string, type = "text/csv;charset=utf-8") {
  const blob = new Blob(["﻿" + text], { type });
  const a = document.createElement("a");
  a.href = URL.createObjectURL(blob);
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 1000);
}

/** DOMjudge result name (i18n key under "result.") and CSS class. */
export function djResult(s: Pick<Submission, "state" | "outcome" | "label">): { key: string; cls: string } {
  if (s.label === "TOO-LATE") return { key: "too-late", cls: "late" };
  if (s.state !== "done") return { key: "pending", cls: "queued" };
  if (s.label === "") return { key: "submitted", cls: "queued" };
  if (s.label === "OK") return { key: "compiled", cls: "correct" };
  switch (s.outcome) {
    case "accepted":
      return { key: "correct", cls: "correct" };
    case "compile_error":
      return { key: "compiler-error", cls: "ce" };
    case "error":
      return { key: "judge-error", cls: "queued" };
    case "rejected": {
      const m: Record<string, string> = { TLE: "timelimit", MLE: "run-error", RE: "run-error", OLE: "output-limit", NO: "no-output" };
      return { key: m[s.label] ?? "wrong-answer", cls: "incorrect" };
    }
    default:
      return { key: "pending", cls: "queued" };
  }
}

/** Which problem a file name refers to (DOMjudge: base name = label or name). */
export function guessProblem(file: string, problems: { name: string; letter: string }[]): string | null {
  const base = file.replace(/\.[^.]*$/, "").toLowerCase();
  const p = problems.find((x) => x.letter.toLowerCase() === base) ?? problems.find((x) => x.name.toLowerCase() === base);
  return p?.name ?? null;
}

export function guessLanguage(file: string): "c" | "cpp" | null {
  if (/\.c$/i.test(file)) return "c";
  if (/\.(cpp|cc|cxx|c\+\+)$/i.test(file)) return "cpp";
  return null;
}
