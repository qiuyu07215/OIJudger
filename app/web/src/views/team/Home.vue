<script setup lang="ts">
// DOMjudge team page: own scoreboard row on top (kept up to date during
// the freeze, rank "?"), submissions on the left, clarifications and own
// clarification requests on the right.
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { NButton, NModal, NCard } from "naive-ui";
import { useI18n } from "vue-i18n";
import { api, type Clarification, type Scoreboard, type Submission } from "../../api";
import { browserNotify, loadSeen, markSeen, onNotice, seen, state, throttle } from "../../store";
import { unread } from "../../unread";
import { clock, djResult, fmtScore } from "../../util";
import BoardTable from "../../components/BoardTable.vue";
import ClarSubject from "../../components/ClarSubject.vue";
import PBadge from "../../components/PBadge.vue";
import RequestClarDialog from "../../components/RequestClarDialog.vue";
import Result from "../../components/Result.vue";
import SubmissionModal from "../../components/SubmissionModal.vue";

const { t } = useI18n();
const row = ref<Scoreboard | null>(null);
const subs = ref<Submission[]>([]);
const clars = ref<Clarification[]>([]);
const ioi = computed(() => state.info?.format === "ioi");

const loadRow = throttle(async () => (row.value = await api.teamRow().catch(() => null)), 1000);
const loadSubs = throttle(async () => {
  const before = new Map(subs.value.map((s) => [s.id, s.state]));
  subs.value = (await api.submissions().catch(() => [])).reverse();
  // Notify about results that just came in.
  for (const s of subs.value) {
    if (before.get(s.id) && before.get(s.id) !== "done" && s.state === "done") {
      const p = state.info?.problems.find((x) => x.name === s.problem);
      browserNotify(`${p?.letter ?? s.problem}: ${t(`result.${djResult(s).key}`)}`, `s${s.id}`);
    }
  }
}, 600);
const loadClars = throttle(async () => (clars.value = await api.clarifications().catch(() => [])), 600);

const off = onNotice((n) => {
  if (n.type === "submission") {
    loadSubs();
    loadRow();
  } else if (n.type === "scoreboard" || n.type === "contest") loadRow();
  else if (n.type === "clarification" || n.type === "announcement") loadClars();
});
onMounted(() => {
  loadSeen();
  unread.clar = 0;
  loadRow();
  loadSubs();
  loadClars();
});
onBeforeUnmount(off);

const me = computed(() => state.me?.id ?? null);
/** Received: jury messages and answers (own requests, or public ones). */
const received = computed(() =>
  clars.value
    .filter((c) => (c.from_account === null || c.answered_ms !== null) && (c.answer || c.question))
    .filter((c) => c.from_account === null || c.from_account === me.value || c.public)
    .map((c) => ({ ...c, time: c.answered_ms ?? c.at_ms }))
    .sort((a, b) => b.time - a.time),
);
const requests = computed(() => clars.value.filter((c) => c.from_account === me.value).sort((a, b) => b.at_ms - a.at_ms));

const openSub = ref<number | null>(null);
function viewSub(s: Submission) {
  if (s.state === "done") markSeen(s.id);
  openSub.value = s.id;
}
const openClar = ref<(Clarification & { time: number }) | null>(null);
function viewClar(c: Clarification & { time: number }) {
  markSeen(-c.id);
  openClar.value = c;
}
const asking = ref(false);
const unseenSub = (s: Submission) => s.state === "done" && !seen.has(s.id);
</script>

<template>
  <div class="dj-container">
    <div v-if="row && row.rows.length" class="own">
      <BoardTable :board="row" :highlight="me" compact />
      <div v-if="row.frozen" class="muted small center">{{ t("team.frozenRank") }}</div>
    </div>

    <div class="cols">
      <section>
        <div class="dj-section">{{ t("team.submissions") }}</div>
        <table class="dj">
          <thead>
            <tr>
              <th>{{ t("team.time") }}</th>
              <th>{{ t("team.problem") }}</th>
              <th>{{ t("team.lang") }}</th>
              <th>{{ t("team.result") }}</th>
              <th v-if="ioi" style="text-align: right">{{ t("team.score") }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="s in subs" :key="s.id" class="link" :class="{ unseen: unseenSub(s), ignored: s.ignored }" @click="viewSub(s)">
              <td class="tabular">{{ clock(s.at_ms).slice(0, 5) }}</td>
              <td><PBadge :problem="s.problem" /></td>
              <td class="langid">{{ s.lang === "c" ? "C" : "CPP" }}</td>
              <td><Result :s="s" /></td>
              <td v-if="ioi" class="tabular" style="text-align: right">{{ s.state === "done" && s.score !== null ? fmtScore(s.score) : "" }}</td>
            </tr>
            <tr v-if="!subs.length"><td :colspan="ioi ? 5 : 4" class="muted center">{{ t("team.noSubmissions") }}</td></tr>
          </tbody>
        </table>
      </section>

      <section>
        <div class="dj-section">{{ t("team.clarifications") }}</div>
        <table class="dj">
          <thead>
            <tr>
              <th>{{ t("team.time") }}</th>
              <th>{{ t("team.from") }}</th>
              <th>{{ t("team.to") }}</th>
              <th>{{ t("team.subject") }}</th>
              <th>{{ t("team.text") }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="c in received" :key="c.id" class="link" :class="{ unseen: !seen.has(-c.id) }" @click="viewClar(c)">
              <td class="tabular">{{ clock(c.time).slice(0, 5) }}</td>
              <td>{{ t("team.jury") }}</td>
              <td>{{ c.to_account === me || (c.from_account === me && !c.public) ? state.me?.display_name || state.me?.username : t("team.all") }}</td>
              <td><ClarSubject :problem="c.problem" /></td>
              <td class="text">{{ (c.answer || c.question).slice(0, 80) }}</td>
            </tr>
            <tr v-if="!received.length"><td colspan="5" class="muted center">{{ t("team.noClarifications") }}</td></tr>
          </tbody>
        </table>

        <div class="dj-section" style="margin-top: 18px">{{ t("team.requests") }}</div>
        <table class="dj">
          <thead>
            <tr>
              <th>{{ t("team.time") }}</th>
              <th>{{ t("team.from") }}</th>
              <th>{{ t("team.to") }}</th>
              <th>{{ t("team.subject") }}</th>
              <th>{{ t("team.text") }}</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="c in requests" :key="c.id" class="link" @click="viewClar({ ...c, time: c.at_ms })">
              <td class="tabular">{{ clock(c.at_ms).slice(0, 5) }}</td>
              <td>{{ state.me?.display_name || state.me?.username }}</td>
              <td>{{ t("team.jury") }}</td>
              <td><ClarSubject :problem="c.problem" /></td>
              <td class="text">{{ c.question.slice(0, 80) }}</td>
            </tr>
            <tr v-if="!requests.length"><td colspan="5" class="muted center">{{ t("team.noRequests") }}</td></tr>
          </tbody>
        </table>
        <n-button size="small" secondary style="margin-top: 10px" @click="asking = true">{{ t("team.requestClar") }}</n-button>
      </section>
    </div>

    <SubmissionModal :id="openSub" @close="openSub = null" />
    <RequestClarDialog :show="asking" @close="asking = false" @sent="loadClars" />
    <n-modal :show="!!openClar" @update:show="(v: boolean) => !v && (openClar = null)">
      <n-card v-if="openClar" :title="t('nav.clarifications')" style="width: min(620px, 94vw)" :bordered="false" size="small" closable @close="openClar = null">
        <div class="meta"><ClarSubject :problem="openClar.problem" /> · <span class="tabular">{{ clock(openClar.time) }}</span></div>
        <div v-if="openClar.question" class="q">
          <div class="muted small">{{ t("clar.question") }}</div>
          <div class="body">{{ openClar.question }}</div>
        </div>
        <div v-if="openClar.answer" class="a">
          <div class="muted small">{{ t("team.jury") }}</div>
          <div class="body">{{ openClar.answer }}</div>
        </div>
      </n-card>
    </n-modal>
  </div>
</template>

<style scoped>
.own {
  margin-bottom: 18px;
}
.cols {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  gap: 24px;
  align-items: start;
}
@media (max-width: 900px) {
  .cols {
    grid-template-columns: 1fr;
  }
}
.center {
  text-align: center;
}
.small {
  font-size: 12px;
}
.text {
  max-width: 280px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.meta {
  margin-bottom: 10px;
}
.q,
.a {
  margin-top: 10px;
}
.a {
  border-left: 3px solid var(--accent);
  padding-left: 10px;
}
.body {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}
</style>
