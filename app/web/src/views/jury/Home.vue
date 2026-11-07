<script setup lang="ts">
// Jury home, as DOMjudge's jury index: status, then links grouped by
// contest stage, with the contest controls next to them.
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { NButton, NProgress, NAlert, useDialog, useMessage } from "naive-ui";
import { useI18n } from "vue-i18n";
import { api, type JuryStatus, type OfflineJudging } from "../../api";
import { isAdmin, now, onNotice, refreshInfo, state, throttle } from "../../store";
import { dateTime, pad } from "../../util";

const { t } = useI18n();
const message = useMessage();
const dialog = useDialog();
const status = ref<JuryStatus | null>(null);
const offline = ref<OfflineJudging | null>(null);
const load = throttle(async () => {
  [status.value, offline.value] = await Promise.all([api.status(), api.offlineStatus()]);
}, 700);
const off = onNotice((n) => {
  if (["queue", "submission", "clarification", "contest"].includes(n.type)) load();
});
onMounted(load);
onBeforeUnmount(off);

const info = computed(() => state.info);
function localNow() {
  const d = new Date(now.value);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}
function startNow() {
  dialog.warning({
    title: t("jury.startNow"),
    content: t("jury.startConfirm"),
    positiveText: t("common.confirm"),
    negativeText: t("common.cancel"),
    onPositiveClick: async () => {
      const cfg = await api.config();
      cfg.start = localNow();
      await api.saveConfig(cfg);
      await refreshInfo();
    },
  });
}
async function act(fn: () => Promise<unknown>, ok?: string) {
  try {
    await fn();
    if (ok) message.success(ok);
    await refreshInfo();
    load();
  } catch (e) {
    message.error(String(e));
  }
}
</script>

<template>
  <div class="dj-container">
    <h1 class="title">{{ info?.title }} · {{ t("jury.title") }}</h1>

    <div class="dj-card status">
      <div>
        <span class="muted">{{ t("jury.judging") }}:</span>
        <b :class="status?.judging_active ? 'ok' : 'bad'"> {{ status?.judging_active ? t("jury.running") : t("jury.stopped") }}</b>
        <span class="muted"> ({{ t("jury.queue", { q: status?.queued ?? 0, r: status?.running ?? 0 }) }})</span>
      </div>
      <div>
        {{ t(`jury.format.${info?.format ?? "icpc"}`) }} · {{ t(`jury.phase.${info?.phase ?? "not_scheduled"}`) }}
        <span v-if="info?.start_ms" class="muted tabular">· {{ dateTime(info.start_ms) }} – {{ info.end_ms ? dateTime(info.end_ms).slice(11) : "" }}</span>
      </div>
      <div class="muted">
        {{ t("jury.online", { n: status?.online ?? 0 }) }} · {{ t("jury.teamsN", { n: status?.accounts ?? 0 }) }} ·
        {{ t("jury.submissionsN", { n: status?.submissions ?? 0 }) }}
      </div>
      <router-link v-if="(status?.unanswered ?? 0) > 0" to="/jury/clarifications" class="bad"><b>{{ t("jury.unanswered", { n: status?.unanswered }) }}</b></router-link>
    </div>

    <div class="cols">
      <div class="dj-card col">
        <div class="dj-card-header"><b>{{ t("jury.before") }}</b></div>
        <ul>
          <li v-if="isAdmin()"><router-link to="/jury/accounts">{{ t("nav.teams") }}</router-link></li>
          <li v-if="isAdmin()"><router-link to="/jury/contest">{{ t("nav.contest") }}</router-link></li>
          <li v-if="isAdmin() && info?.phase !== 'running'"><a href="#" @click.prevent="startNow">{{ t("jury.startNow") }}</a></li>
          <li v-if="isAdmin()"><a href="#" @click.prevent="act(() => api.reload(), t('jury.reloaded'))">{{ t("jury.reload") }}</a></li>
        </ul>
      </div>
      <div class="dj-card col">
        <div class="dj-card-header"><b>{{ t("jury.during") }}</b></div>
        <ul>
          <li><router-link to="/jury/submissions">{{ t("nav.submissions") }}</router-link></li>
          <li><router-link to="/jury/clarifications">{{ t("nav.clarifications") }}</router-link></li>
          <li><router-link to="/balloons">{{ t("nav.balloons") }}</router-link></li>
          <li><router-link to="/print-queue">{{ t("nav.printQueue") }}</router-link></li>
          <li><router-link to="/scoreboard">{{ t("nav.scoreboard") }}</router-link></li>
          <li><a href="/scoreboard?projector=1" target="_blank">{{ t("nav.projector") }}</a></li>
        </ul>
      </div>
      <div class="dj-card col">
        <div class="dj-card-header"><b>{{ t("jury.after") }}</b></div>
        <ul>
          <template v-if="info?.freeze_at">
            <li v-if="info.frozen"><a href="#" @click.prevent="act(() => api.freeze(true))">{{ t("jury.unfreeze") }}</a></li>
            <li v-else-if="now >= info.freeze_at"><a href="#" @click.prevent="act(() => api.freeze(false))">{{ t("jury.refreeze") }}</a></li>
          </template>
          <li v-if="info?.format === 'icpc'"><router-link to="/jury/resolver">{{ t("nav.resolver") }}</router-link></li>
          <template v-if="info?.format === 'oi'">
            <li>
              <a href="#" :class="{ disabled: info.phase !== 'ended' || offline?.running }" @click.prevent="info.phase === 'ended' && !offline?.running && act(() => api.offlineStart())">{{ t("jury.judgeOffline") }}</a>
            </li>
            <li>
              <a href="#" @click.prevent="act(() => api.publish(!info?.results_published))">{{ info.results_published ? t("jury.unpublish") : t("jury.publish") }}</a>
            </li>
          </template>
        </ul>
        <div v-if="offline" class="offline">
          <div class="small">{{ offline.running ? t("jury.offlineProgress", { d: offline.done, t: offline.total }) : t("jury.offlineDone") }}</div>
          <n-progress type="line" :percentage="offline.total ? Math.round((offline.done / offline.total) * 100) : 0" :processing="offline.running" color="#D97757" />
          <n-alert v-if="offline.error" type="error" :show-icon="false">{{ offline.error }}</n-alert>
        </div>
      </div>
    </div>
    <n-button v-if="false" />
  </div>
</template>

<style scoped>
.title {
  font-weight: 500;
  font-size: 24px;
  margin: 4px 0 14px;
}
.status {
  padding: 12px 16px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: 14px;
  margin-bottom: 16px;
}
.ok {
  color: var(--ok);
}
.bad {
  color: var(--bad);
}
.cols {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
  gap: 14px;
}
.col ul {
  margin: 0;
  padding: 10px 14px 12px 32px;
  line-height: 2;
}
.col a {
  color: var(--text);
}
.col a.disabled {
  color: var(--text3);
  cursor: not-allowed;
}
.offline {
  padding: 0 14px 12px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.small {
  font-size: 12.5px;
}
</style>
