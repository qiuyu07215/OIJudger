<script setup lang="ts">
// Online contest: run the LAN web server for the open contest; contestants
// use the shown address; the jury interface opens in an app window.
import { onBeforeUnmount, ref, watch } from "vue";
import { NModal, NCard, NButton, NInput, NAlert, NTag, NSpace, useMessage } from "naive-ui";
import { useI18n } from "vue-i18n";
import { api, type OnlineView } from "../api";

const props = defineProps<{ show: boolean }>();
const emit = defineEmits<{ "update:show": [boolean] }>();
const { t } = useI18n();
const message = useMessage();
const v = ref<OnlineView | null>(null);
const bind = ref("");
const busy = ref(false);
let timer: ReturnType<typeof setInterval> | undefined;

async function refresh() {
  v.value = await api.onlineStatus().catch(() => null);
  if (v.value && !bind.value) bind.value = v.value.bind;
}
watch(
  () => props.show,
  (s) => {
    clearInterval(timer);
    if (s) {
      refresh();
      timer = setInterval(refresh, 2000);
    }
  },
  { immediate: true },
);
onBeforeUnmount(() => clearInterval(timer));

async function run(fn: () => Promise<unknown>) {
  busy.value = true;
  try {
    await fn();
    await refresh();
  } catch (e) {
    message.error(String(e), { duration: 8000 });
  } finally {
    busy.value = false;
  }
}
async function copy(s: string) {
  await navigator.clipboard.writeText(s);
  message.success(t("online.copied"));
}
</script>

<template>
  <n-modal :show="show" @update:show="(x: boolean) => emit('update:show', x)">
    <n-card :title="t('online.title')" style="width: min(620px, 94vw)" :bordered="false" size="small" closable @close="emit('update:show', false)">
      <div class="muted intro">{{ t("online.intro") }}</div>
      <template v-if="!v?.running">
        <div class="row">
          <span class="label">{{ t("online.bind") }}</span>
          <n-input v-model:value="bind" placeholder="0.0.0.0:8080" style="max-width: 220px" />
          <n-button type="primary" :loading="busy" @click="run(() => api.onlineStart(bind))">{{ t("online.start") }}</n-button>
        </div>
        <div class="muted small">{{ t("online.firewall") }}</div>
      </template>
      <template v-else>
        <n-space align="center" style="margin-bottom: 10px">
          <n-tag type="success" :bordered="false">{{ t("online.running") }}</n-tag>
          <span class="muted tabular">{{ v.addr }} · {{ v.format.toUpperCase() }}</span>
          <span class="muted">{{ t("online.stats", { o: v.online, q: v.queued, r: v.running_jobs }) }}</span>
        </n-space>
        <div class="label">{{ t("online.urls") }}</div>
        <div v-for="u in v.urls" :key="u" class="url">
          <span class="mono big">{{ u }}</span>
          <n-button size="tiny" quaternary @click="copy(u)">{{ t("online.copy") }}</n-button>
        </div>
        <div class="muted small">{{ t("online.local", { u: v.local_url }) }}</div>
        <n-alert v-if="v.admin_password" type="warning" :show-icon="false" class="alert">
          {{ t("online.adminPw") }} <b class="mono">admin / {{ v.admin_password }}</b>
        </n-alert>
        <n-alert v-if="v.judging_error" type="error" :show-icon="false" class="alert">{{ t("online.judgingError", { e: v.judging_error }) }}</n-alert>
        <div class="actions">
          <n-button @click="run(() => api.onlineStop())">{{ t("online.stop") }}</n-button>
          <n-button type="primary" @click="run(() => api.onlineOpenJury())">{{ t("online.openJury") }}</n-button>
        </div>
      </template>
    </n-card>
  </n-modal>
</template>

<style scoped>
.intro {
  font-size: 13px;
  margin-bottom: 14px;
}
.row {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 6px;
}
.label {
  font-weight: 600;
  margin: 8px 0 4px;
}
.url {
  display: flex;
  align-items: center;
  gap: 8px;
}
.big {
  font-size: 16px;
}
.small {
  font-size: 12.5px;
  margin-top: 4px;
}
.alert {
  margin-top: 10px;
}
.actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 14px;
}
</style>
