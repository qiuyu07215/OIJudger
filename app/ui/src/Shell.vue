<script setup lang="ts">
import { computed, h, onBeforeUnmount, onMounted, ref, watch, type Component } from "vue";
import { NButton, NDropdown, NIcon, NTabs, NTab, NBadge, NProgress, useDialog, useMessage, useNotification, type DropdownOption } from "naive-ui";
import { useI18n } from "vue-i18n";
import {
  PlayerPlay, PlayerTrackNext, PlayerStop, Refresh, Folder, FileExport, Settings, ListCheck, Users,
} from "@vicons/tabler";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { openPath } from "@tauri-apps/plugin-opener";
import { api } from "./api";
import { state, judging, staleCount, openContest, reloadContest, closeContest, applyUpdates, appendLog } from "./store";
import { setLocale } from "./i18n";
import { judge, exportAs, pickContestFolder, pickCdf, type JudgeKind } from "./actions";
import Welcome from "./views/Welcome.vue";
import Results from "./views/Results.vue";
import Problems from "./views/Problems.vue";
import Stats from "./views/Stats.vue";
import Plagiarism from "./views/Plagiarism.vue";
import SettingsModal from "./views/SettingsModal.vue";
import JudgingDialog from "./components/JudgingDialog.vue";

const { t } = useI18n();
const message = useMessage();
const dialog = useDialog();
const notification = useNotification();
const tab = ref<"summary" | "results" | "stats" | "plagiarism">("results");
const showSettings = ref(false);
const selected = ref<string[]>([]);
const problemsRef = ref<InstanceType<typeof Problems> | null>(null);
const unlisten: (() => void)[] = [];

async function open(path: string | null) {
  if (!path) return;
  try {
    await openContest(path);
    tab.value = state.contest?.problem.length ? "results" : "summary";
  } catch (e) {
    message.error(t("msg.openFailed", { e: String(e) }), { duration: 6000 });
  }
}

const runJudge = (k: JudgeKind) => judge(k, selected.value, message);

// ---- menu bar -------------------------------------------------------------
const icon = (c: Component) => () => h(NIcon, null, { default: () => h(c) });
const hasContest = computed(() => !!state.contest);

const menus = computed<{ key: string; label: string; options: DropdownOption[] }[]>(() => [
  {
    key: "file",
    label: t("menu.file"),
    options: [
      { label: t("menu.open"), key: "open", disabled: judging.value },
      { label: t("menu.importCdf"), key: "importCdf", disabled: judging.value },
      {
        label: t("menu.recent"),
        key: "recent",
        disabled: judging.value || !state.settings?.recent?.length,
        children: (state.settings?.recent ?? []).map((r) => ({ label: r, key: `recent:${r}` })),
      },
      { type: "divider", key: "d1" },
      { label: t("menu.openDir"), key: "openDir", disabled: !hasContest.value },
      { label: t("menu.close"), key: "close", disabled: !hasContest.value || judging.value },
    ],
  },
  {
    key: "contest",
    label: t("menu.contest"),
    options: [
      { label: t("menu.reload"), key: "reload", disabled: !hasContest.value || judging.value, icon: icon(Refresh) },
      { label: t("menu.detect"), key: "detect", disabled: !hasContest.value || judging.value },
      {
        label: t("menu.export"),
        key: "export",
        disabled: !hasContest.value,
        icon: icon(FileExport),
        children: [
          { label: t("stats.exportHtml"), key: "export:html" },
          { label: t("stats.exportHtm"), key: "export:htm" },
          { label: t("stats.exportXlsx"), key: "export:xlsx" },
          { label: t("stats.exportCsv"), key: "export:csv" },
        ],
      },
    ],
  },
  {
    key: "judge",
    label: t("menu.judge"),
    options: [
      { label: t("menu.judgeAll"), key: "judge:all", disabled: !hasContest.value || judging.value, icon: icon(PlayerPlay) },
      { label: t("menu.judgeStale"), key: "judge:stale", disabled: !hasContest.value || judging.value, icon: icon(PlayerTrackNext) },
      { label: t("menu.judgeSelected"), key: "judge:selected", disabled: !hasContest.value || judging.value, icon: icon(Users) },
      { type: "divider", key: "d2" },
      { label: t("menu.stop"), key: "stop", disabled: !judging.value, icon: icon(PlayerStop) },
      { label: t("menu.showJudging"), key: "showJudging", disabled: !state.progress },
    ],
  },
  {
    key: "tools",
    label: t("menu.tools"),
    options: [
      { label: t("menu.settings"), key: "settings", icon: icon(Settings) },
      { label: t("menu.selftest"), key: "selftest" },
      { label: t("menu.plagiarism"), key: "plagiarism", disabled: !hasContest.value },
      { type: "divider", key: "d3" },
      { label: t("menu.clearCache"), key: "clearCache", disabled: !hasContest.value || judging.value },
    ],
  },
  { key: "help", label: t("menu.help"), options: [{ label: t("menu.about"), key: "about" }] },
]);

const fmtMB = (b: number) => (b / 1e6).toFixed(b < 1e7 ? 1 : 0);
async function clearCache() {
  let usage: { files: number; bytes: number };
  try {
    usage = await api.cacheUsage();
  } catch (e) {
    message.error(String(e));
    return;
  }
  if (!usage.files) {
    message.info(t("cache.empty"));
    return;
  }
  dialog.warning({
    title: t("cache.title"),
    content: t("cache.confirm", { mb: fmtMB(usage.bytes), n: usage.files }),
    positiveText: t("common.delete"),
    negativeText: t("common.cancel"),
    onPositiveClick: async () => {
      try {
        const r = await api.clearCache();
        message.success(t("cache.done", { mb: fmtMB(r.bytes) }));
      } catch (e) {
        message.error(String(e), { duration: 8000 });
      }
    },
  });
}

async function onMenu(key: string) {
  if (key.startsWith("recent:")) return open(key.slice(7));
  if (key.startsWith("export:")) return exportAs(key.slice(7) as "html", message);
  if (key.startsWith("judge:")) return runJudge(key.slice(6) as JudgeKind);
  switch (key) {
    case "open": return open(await pickContestFolder());
    case "importCdf": return open(await pickCdf());
    case "openDir": return openPath(state.root);
    case "close": return closeContest();
    case "reload": return reloadContest();
    case "detect":
      tab.value = "summary";
      setTimeout(() => problemsRef.value?.detect(), 50);
      return;
    case "stop": return api.cancelJudge();
    case "showJudging": state.showJudging = true; return;
    case "settings": case "selftest": showSettings.value = true; return;
    case "plagiarism": tab.value = "plagiarism"; return;
    case "clearCache": return clearCache();
    case "about":
      dialog.info({ title: t("menu.about"), content: t("menu.aboutText", { v: state.info?.version ?? "" }), positiveText: t("common.ok") });
      return;
  }
}

// ---- status bar -------------------------------------------------------------
const compiling = computed(() => !!state.progress?.running && !state.progress.total && state.progress.compile_total > 0);
const pct = computed(() => {
  const p = state.progress;
  if (!p) return 0;
  if (compiling.value) return Math.round((p.compile_done / p.compile_total) * 100);
  return p.total ? Math.round((p.done / p.total) * 100) : 0;
});

onMounted(async () => {
  state.info = await api.appInfo();
  state.settings = await api.getSettings();
  setLocale(state.settings.language);
  state.progress = await api.judgeStatus();
  unlisten.push(
    await api.onProgress((p) => {
      applyUpdates(p.updates);
      appendLog(p.log);
      const wasRunning = state.progress?.running;
      state.progress = p;
      if (wasRunning && !p.running) {
        if (p.error) notification.error({ title: t("msg.judgeFailed", { e: "" }), content: p.error, duration: 10000 });
        else if (!state.showJudging)
          notification.success({ title: t("progress.finished"), content: t("progress.done", { done: p.done, total: p.total }), duration: 4000 });
        reloadContest().catch(() => {});
      }
    }),
  );
  unlisten.push(
    await getCurrentWebview().onDragDropEvent((e) => {
      if (e.payload.type === "drop" && e.payload.paths.length && !judging.value) open(e.payload.paths[0]);
    }),
  );
});
onBeforeUnmount(() => unlisten.forEach((f) => f()));

watch(
  () => state.settings?.language,
  (l) => l && setLocale(l),
);
</script>

<template>
  <div class="shell">
    <!-- menu bar -->
    <div class="menubar">
      <n-dropdown v-for="m in menus" :key="m.key" trigger="click" :options="m.options" size="small" @select="onMenu">
        <button class="menu-item">{{ m.label }}</button>
      </n-dropdown>
      <div class="title serif">{{ state.contest?.title || "OIJudger" }}</div>
    </div>

    <template v-if="state.contest">
      <!-- tool bar -->
      <div class="toolbar">
        <n-button size="small" quaternary :disabled="judging" @click="runJudge('all')">
          <template #icon><n-icon><PlayerPlay /></n-icon></template>{{ t("toolbar.judgeAll") }}
        </n-button>
        <n-badge :value="staleCount" :max="9999" :show="staleCount > 0 && !judging" :offset="[-4, 4]">
          <n-button size="small" type="primary" :disabled="judging" @click="runJudge('stale')">
            <template #icon><n-icon><PlayerTrackNext /></n-icon></template>{{ t("toolbar.judgeStale") }}
          </n-button>
        </n-badge>
        <n-button size="small" quaternary :disabled="judging || !selected.length" @click="runJudge('selected')">
          <template #icon><n-icon><ListCheck /></n-icon></template>{{ t("toolbar.judgeSelected") }}
        </n-button>
        <n-button size="small" quaternary type="error" :disabled="!judging" @click="api.cancelJudge()">
          <template #icon><n-icon><PlayerStop /></n-icon></template>{{ t("toolbar.stop") }}
        </n-button>
        <span class="sep" />
        <n-button size="small" quaternary :disabled="judging" @click="reloadContest()">
          <template #icon><n-icon><Refresh /></n-icon></template>{{ t("toolbar.reload") }}
        </n-button>
        <n-button size="small" quaternary @click="openPath(state.root)">
          <template #icon><n-icon><Folder /></n-icon></template>{{ t("toolbar.openDir") }}
        </n-button>
        <n-dropdown
          :options="(menus[1].options.find((o) => o.key === 'export')?.children as DropdownOption[])"
          size="small"
          @select="onMenu"
        >
          <n-button size="small" quaternary>
            <template #icon><n-icon><FileExport /></n-icon></template>{{ t("toolbar.export") }}
          </n-button>
        </n-dropdown>
        <span class="sep" />
        <n-button size="small" quaternary @click="showSettings = true">
          <template #icon><n-icon><Settings /></n-icon></template>{{ t("toolbar.settings") }}
        </n-button>
      </div>

      <!-- tabs -->
      <n-tabs v-model:value="tab" type="line" size="small" :tabs-padding="12" class="tabs">
        <n-tab name="summary">{{ t("nav.summary") }}</n-tab>
        <n-tab name="results">{{ t("nav.results") }}</n-tab>
        <n-tab name="stats">{{ t("nav.stats") }}</n-tab>
        <n-tab name="plagiarism">{{ t("nav.plagiarism") }}</n-tab>
      </n-tabs>
      <div class="content">
        <div v-show="tab === 'results'" class="page"><Results v-model:selected="selected" /></div>
        <div v-if="tab === 'summary'" class="page"><Problems ref="problemsRef" /></div>
        <div v-if="tab === 'stats'" class="page"><Stats /></div>
        <div v-if="tab === 'plagiarism'" class="page"><Plagiarism /></div>
      </div>

      <!-- status bar -->
      <div class="statusbar">
        <span class="path" :title="state.root">{{ state.root }}</span>
        <span>{{ t("statusbar.contestants", { n: state.contestants.length }) }}</span>
        <span>{{ t("statusbar.problems", { n: state.contest.problem.length }) }}</span>
        <span v-if="staleCount && !judging" class="accent">{{ t("statusbar.stale", { n: staleCount }) }}</span>
        <span class="grow" />
        <button v-if="state.progress && (judging || state.progress.total)" class="judge-status" :title="t('statusbar.show')" @click="state.showJudging = true">
          <span>{{
            compiling
              ? t("judging.compiling", { done: state.progress.compile_done, total: state.progress.compile_total })
              : judging
                ? t("statusbar.judging", { done: state.progress.done, total: state.progress.total }) +
                  (state.progress.compile_done < state.progress.compile_total
                    ? " · " + t("judging.compiledShort", { done: state.progress.compile_done, total: state.progress.compile_total })
                    : "") +
                  (state.progress.compile_paused ? " · " + t("judging.paused") : "")
                : t("progress.finished")
          }}</span>
          <n-progress type="line" :percentage="pct" :show-indicator="false" :height="6" :processing="judging" color="#D97757" style="width: 140px" />
        </button>
        <span v-else>{{ t("statusbar.ready") }}</span>
      </div>
    </template>
    <Welcome v-else class="content" @open="open" @settings="showSettings = true" />
  </div>
  <SettingsModal v-model:show="showSettings" />
  <JudgingDialog />
</template>

<style scoped>
.shell {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--app);
}
.menubar {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 2px 8px;
  background: var(--panel);
  border-bottom: 1px solid var(--divider);
  user-select: none;
}
.menu-item {
  font: inherit;
  font-size: 13px;
  color: var(--text2);
  background: none;
  border: 0;
  padding: 4px 10px;
  border-radius: 6px;
  cursor: pointer;
}
.menu-item:hover {
  background: color-mix(in srgb, var(--text) 7%, transparent);
  color: var(--text);
}
.title {
  margin-left: auto;
  font-size: 14px;
  color: var(--text2);
  padding-right: 6px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.toolbar {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 6px 10px;
  background: var(--panel);
  border-bottom: 1px solid var(--divider);
  flex-wrap: wrap;
}
.sep {
  width: 1px;
  height: 18px;
  background: var(--divider);
  margin: 0 6px;
}
.tabs {
  flex: none;
  background: var(--app);
}
.content {
  flex: 1;
  min-height: 0;
  position: relative;
}
.page {
  position: absolute;
  inset: 0;
}
.statusbar {
  display: flex;
  align-items: center;
  gap: 16px;
  padding: 3px 12px;
  font-size: 12px;
  color: var(--text3);
  background: var(--panel);
  border-top: 1px solid var(--divider);
  white-space: nowrap;
}
.statusbar .path {
  max-width: 40%;
  overflow: hidden;
  text-overflow: ellipsis;
}
.statusbar .grow {
  flex: 1;
}
.accent {
  color: var(--accent);
}
.judge-status {
  display: flex;
  align-items: center;
  gap: 8px;
  font: inherit;
  color: var(--text2);
  background: none;
  border: 0;
  cursor: pointer;
  padding: 0;
}
</style>
