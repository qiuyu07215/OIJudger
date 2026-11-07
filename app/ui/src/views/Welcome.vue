<script setup lang="ts">
import { NButton, NIcon, NSpin } from "naive-ui";
import { Folder, FileImport, Settings, Clock } from "@vicons/tabler";
import { useI18n } from "vue-i18n";
import { state } from "../store";
import { pickCdf, pickContestFolder } from "../actions";

const emit = defineEmits<{ open: [path: string | null]; settings: [] }>();
const { t } = useI18n();
</script>

<template>
  <div class="welcome">
    <n-spin :show="state.busy">
      <div class="card">
        <div class="brand">
          <svg viewBox="0 0 64 64" width="48" height="48" aria-hidden="true">
            <rect x="4" y="4" width="56" height="56" rx="14" fill="#D97757" />
            <path d="M18 33 l9 9 l19 -20" stroke="#FAF9F5" stroke-width="7" fill="none" stroke-linecap="round" stroke-linejoin="round" />
          </svg>
          <div>
            <h1 class="serif">{{ t("app.name") }}</h1>
            <div class="tagline">{{ t("app.tagline") }}</div>
          </div>
        </div>

        <div class="actions">
          <n-button type="primary" size="large" @click="async () => emit('open', await pickContestFolder())">
            <template #icon><n-icon><Folder /></n-icon></template>
            {{ t("welcome.open") }}
          </n-button>
          <n-button size="large" @click="async () => emit('open', await pickCdf())">
            <template #icon><n-icon><FileImport /></n-icon></template>
            {{ t("welcome.importCdf") }}
          </n-button>
        </div>
        <div class="hint">{{ t("welcome.hint") }}</div>

        <template v-if="state.settings?.recent?.length">
          <div class="section">{{ t("welcome.recent") }}</div>
          <div class="recent">
            <button v-for="r in state.settings.recent" :key="r" class="recent-item" @click="emit('open', r)">
              <n-icon class="ri"><Clock /></n-icon>
              <span class="rp">{{ r }}</span>
            </button>
          </div>
        </template>

        <div class="section">{{ t("welcome.layoutTitle") }}</div>
        <pre class="code">{{ t("welcome.layout") }}</pre>
        <div class="hint" style="text-align: left; margin-top: 8px">{{ t("welcome.noData") }}</div>

        <div style="text-align: right; margin-top: 16px">
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
  padding: 32px 16px;
  box-sizing: border-box;
  background: var(--app);
}
.card {
  width: min(600px, 100%);
}
.brand {
  display: flex;
  align-items: center;
  gap: 16px;
  margin-bottom: 28px;
}
h1 {
  font-size: 34px;
  font-weight: 500;
  margin: 0;
  letter-spacing: -0.3px;
  color: var(--text);
}
.tagline {
  color: var(--text3);
  margin-top: 2px;
}
.actions {
  display: flex;
  gap: 10px;
  flex-wrap: wrap;
}
.actions :deep(.n-button) {
  flex: 1;
  min-width: 220px;
}
.hint {
  font-size: 12px;
  color: var(--text3);
  margin-top: 10px;
}
.section {
  font-weight: 600;
  color: var(--text2);
  margin: 26px 0 8px;
}
.recent {
  border: 1px solid var(--border);
  border-radius: 10px;
  overflow: hidden;
  background: var(--surface);
}
.recent-item {
  display: flex;
  align-items: center;
  gap: 10px;
  width: 100%;
  font: inherit;
  font-size: 13px;
  text-align: left;
  color: var(--text2);
  background: none;
  border: 0;
  border-bottom: 1px solid var(--divider);
  padding: 9px 14px;
  cursor: pointer;
}
.recent-item:last-child {
  border-bottom: 0;
}
.recent-item:hover {
  background: var(--accent-soft);
  color: var(--text);
}
.ri {
  color: var(--text3);
}
.rp {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
