<script setup lang="ts">
import { NButton, NIcon, NList, NListItem, NEllipsis, NSpin } from "naive-ui";
import { Folder, Settings } from "@vicons/tabler";
import { useI18n } from "vue-i18n";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { state } from "../store";

const emit = defineEmits<{ open: [path: string]; settings: [] }>();
const { t } = useI18n();

async function pick() {
  const dir = await openDialog({ directory: true, multiple: false });
  if (typeof dir === "string") emit("open", dir);
}
</script>

<template>
  <div class="welcome">
    <n-spin :show="state.busy">
      <div class="card">
        <div class="logo">
          <svg viewBox="0 0 64 64" width="56" height="56" aria-hidden="true">
            <rect x="4" y="4" width="56" height="56" rx="14" fill="#2f7d4f" />
            <path d="M18 33 l9 9 l19 -20" stroke="#fff" stroke-width="7" fill="none" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
          <div>
            <div class="name">{{ t("app.name") }}</div>
            <div class="tagline">{{ t("app.tagline") }}</div>
          </div>
        </div>
        <n-button type="primary" size="large" block @click="pick">
          <template #icon><n-icon><Folder /></n-icon></template>
          {{ t("welcome.open") }}
        </n-button>
        <div class="hint">{{ t("welcome.hint") }}</div>

        <template v-if="state.settings?.recent?.length">
          <div class="section">{{ t("welcome.recent") }}</div>
          <n-list hoverable clickable bordered size="small">
            <n-list-item v-for="r in state.settings.recent" :key="r" @click="emit('open', r)">
              <n-ellipsis style="max-width: 100%">{{ r }}</n-ellipsis>
            </n-list-item>
          </n-list>
        </template>

        <div class="section">{{ t("welcome.layoutTitle") }}</div>
        <pre class="code">{{ t("welcome.layout") }}</pre>
        <div class="hint" style="text-align: left">{{ t("welcome.noData") }}</div>

        <div style="text-align: right; margin-top: 12px">
          <n-button quaternary size="small" @click="emit('settings')">
            <template #icon><n-icon><Settings /></n-icon></template>
            {{ t("toolbar.settings") }}
          </n-button>
        </div>
      </div>
    </n-spin>
  </div>
</template>

<style scoped>
.welcome {
  height: 100%;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: auto;
  padding: 24px 16px;
  box-sizing: border-box;
}
.card {
  width: min(560px, 100%);
}
.logo {
  display: flex;
  align-items: center;
  gap: 14px;
  margin-bottom: 24px;
}
.name {
  font-size: 26px;
  font-weight: 700;
  letter-spacing: 0.3px;
}
.tagline {
  opacity: 0.65;
}
.hint {
  font-size: 12px;
  opacity: 0.6;
  margin-top: 8px;
  text-align: center;
}
.section {
  font-weight: 600;
  margin: 22px 0 8px;
}
</style>
