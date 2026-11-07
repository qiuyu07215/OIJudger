<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from "vue";
import { NLayout, NLayoutHeader, NLayoutContent, NButton, NSpace, NTabs, NTab, NTooltip, NBadge, NDropdown, NIcon, useMessage, useNotification } from "naive-ui";
import { useI18n } from "vue-i18n";
import { ChevronDown, Folder, Logout, PlayerPlay, PlayerStop, Refresh, Settings } from "@vicons/tabler";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { openPath } from "@tauri-apps/plugin-opener";
import { api } from "./api";
import { state, judging, staleCount, openContest, reloadContest, closeContest, applyUpdates } from "./store";
import { setLocale } from "./i18n";
import Welcome from "./views/Welcome.vue";
import Results from "./views/Results.vue";
import Problems from "./views/Problems.vue";
import Stats from "./views/Stats.vue";
import Plagiarism from "./views/Plagiarism.vue";
import SettingsModal from "./views/SettingsModal.vue";
import ProgressPanel from "./components/ProgressPanel.vue";

const { t } = useI18n();
const message = useMessage();
const notification = useNotification();
const tab = ref("results");
const showSettings = ref(false);
const selected = ref<string[]>([]);
const unlisten: (() => void)[] = [];

async function open(path: string) {
  try {
    await openContest(path);
    tab.value = "results";
  } catch (e) {
    message.error(t("msg.openFailed", { e: String(e) }), { duration: 6000 });
  }
}

async function judge(kind: "stale" | "all" | "selected") {
  try {
    if (kind === "selected") {
      if (!selected.value.length) return message.warning(t("msg.selectFirst"));
      await api.startJudge({ contestants: selected.value });
    } else {
      if (kind === "stale" && staleCount.value === 0) return message.info(t("msg.nothingToJudge"));
      await api.startJudge({ only_stale: kind === "stale" });
    }
  } catch (e) {
    message.error(t("msg.judgeFailed", { e: String(e) }));
  }
}

const moreOptions = [
  { label: () => t("toolbar.judgeAll"), key: "all" },
  { label: () => t("toolbar.judgeSelected"), key: "selected" },
];

onMounted(async () => {
  state.info = await api.appInfo();
  state.settings = await api.getSettings();
  setLocale(state.settings.language);
  state.progress = await api.judgeStatus();
  unlisten.push(
    await api.onProgress((p) => {
      applyUpdates(p.updates);
      const wasRunning = state.progress?.running;
      state.progress = p;
      if (wasRunning && !p.running) {
        if (p.error) notification.error({ title: t("msg.judgeFailed", { e: "" }), content: p.error, duration: 10000 });
        else notification.success({ title: t("progress.finished"), content: t("progress.done", { done: p.done, total: p.total }), duration: 4000 });
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
  <n-layout v-if="!state.contest" style="height: 100%">
    <Welcome @open="open" @settings="showSettings = true" />
  </n-layout>
  <n-layout v-else style="height: 100%" content-style="display:flex;flex-direction:column;height:100%">
    <n-layout-header bordered style="padding: 8px 16px; display: flex; align-items: center; gap: 16px">
      <div style="min-width: 0; flex: 1">
        <div style="font-weight: 600; font-size: 16px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis">
          {{ state.contest.title || "OIJudger" }}
        </div>
        <div style="font-size: 12px; opacity: 0.6; white-space: nowrap; overflow: hidden; text-overflow: ellipsis">{{ state.root }}</div>
      </div>
      <n-space :size="8" align="center" :wrap="false">
        <n-badge :value="staleCount" :max="9999" :show="staleCount > 0 && !judging">
          <n-button type="primary" :disabled="judging" @click="judge('stale')"><template #icon><n-icon><PlayerPlay /></n-icon></template>{{ t("toolbar.judgeStale") }}</n-button>
        </n-badge>
        <n-dropdown :options="moreOptions" :disabled="judging" @select="(k: 'all' | 'selected') => judge(k)">
          <n-button :disabled="judging"><template #icon><n-icon><ChevronDown /></n-icon></template></n-button>
        </n-dropdown>
        <n-button v-if="judging" type="error" ghost @click="api.cancelJudge()"><template #icon><n-icon><PlayerStop /></n-icon></template>{{ t("toolbar.stop") }}</n-button>
        <n-tooltip>
          <template #trigger>
            <n-button quaternary :disabled="judging" @click="reloadContest()"><template #icon><n-icon><Refresh /></n-icon></template></n-button>
          </template>
          {{ t("toolbar.reload") }}
        </n-tooltip>
        <n-tooltip>
          <template #trigger>
            <n-button quaternary @click="openPath(state.root)"><template #icon><n-icon><Folder /></n-icon></template></n-button>
          </template>
          {{ t("toolbar.openDir") }}
        </n-tooltip>
        <n-tooltip>
          <template #trigger>
            <n-button quaternary @click="showSettings = true"><template #icon><n-icon><Settings /></n-icon></template></n-button>
          </template>
          {{ t("toolbar.settings") }}
        </n-tooltip>
        <n-tooltip>
          <template #trigger>
            <n-button quaternary :disabled="judging" @click="closeContest()"><template #icon><n-icon><Logout /></n-icon></template></n-button>
          </template>
          {{ t("toolbar.closeContest") }}
        </n-tooltip>
      </n-space>
    </n-layout-header>
    <n-layout-content style="flex: 1; min-height: 0" content-style="height:100%;display:flex;flex-direction:column">
      <n-tabs v-model:value="tab" type="line" :tabs-padding="16" style="flex: none">
        <n-tab name="results">{{ t("nav.results") }}</n-tab>
        <n-tab name="problems">{{ t("nav.problems") }}</n-tab>
        <n-tab name="stats">{{ t("nav.stats") }}</n-tab>
        <n-tab name="plagiarism">{{ t("nav.plagiarism") }}</n-tab>
      </n-tabs>
      <div style="flex: 1; min-height: 0; position: relative">
        <div v-show="tab === 'results'" class="page"><Results v-model:selected="selected" /></div>
        <div v-if="tab === 'problems'" class="page"><Problems /></div>
        <div v-if="tab === 'stats'" class="page"><Stats /></div>
        <div v-if="tab === 'plagiarism'" class="page"><Plagiarism /></div>
      </div>
      <ProgressPanel />
    </n-layout-content>
  </n-layout>
  <SettingsModal v-model:show="showSettings" />
</template>

<style scoped>
.page {
  position: absolute;
  inset: 0;
}
</style>
