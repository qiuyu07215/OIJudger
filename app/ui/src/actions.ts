// Actions shared by the menu bar, the tool bar and the pages.
import { save, open as openDialog } from "@tauri-apps/plugin-dialog";
import type { MessageApi } from "naive-ui";
import { api } from "./api";
import { i18n } from "./i18n";
import { state, staleCount, clearLog } from "./store";

const t = (k: string, a?: Record<string, unknown>) => i18n.global.t(k, a ?? {});

export type JudgeKind = "stale" | "all" | "selected";

export async function judge(kind: JudgeKind, selected: string[], message: MessageApi) {
  try {
    if (kind === "selected") {
      if (!selected.length) return message.warning(t("msg.selectFirst"));
      clearLog();
      await api.startJudge({ contestants: selected });
    } else {
      if (kind === "stale" && staleCount.value === 0) return message.info(t("msg.nothingToJudge"));
      clearLog();
      await api.startJudge({ only_stale: kind === "stale" });
    }
    state.showJudging = true;
  } catch (e) {
    message.error(t("msg.judgeFailed", { e: String(e) }));
  }
}

export async function judgePairs(pairs: [string, string][], message: MessageApi) {
  try {
    clearLog();
    await api.startJudge({ pairs });
    state.showJudging = true;
  } catch (e) {
    message.error(t("msg.judgeFailed", { e: String(e) }));
  }
}

export async function exportAs(fmt: "html" | "htm" | "xlsx" | "csv", message: MessageApi) {
  const base = (state.contest?.title || "results").replace(/[\\/:*?"<>|]/g, "_");
  const path = await save({ defaultPath: `${state.root}\\${base}.${fmt}`, filters: [{ name: fmt.toUpperCase(), extensions: [fmt] }] });
  if (!path) return;
  try {
    await api.exportResults(fmt, path);
    message.success(t("stats.exported", { p: path }));
  } catch (e) {
    message.error(String(e));
  }
}

export async function pickContestFolder(): Promise<string | null> {
  const dir = await openDialog({ directory: true, multiple: false });
  return typeof dir === "string" ? dir : null;
}

export async function pickCdf(): Promise<string | null> {
  const f = await openDialog({ multiple: false, filters: [{ name: "LemonLime", extensions: ["cdf"] }] });
  return typeof f === "string" ? f : null;
}
