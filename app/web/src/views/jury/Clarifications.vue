<script setup lang="ts">
// DOMjudge jury clarifications: new requests, handled requests and general
// clarifications; answer a request (to the team or to all) or send a new
// clarification.
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { NButton, NModal, NCard, NInput, NRadioGroup, NRadio, NSelect, useMessage } from "naive-ui";
import { useI18n } from "vue-i18n";
import { api, type Account, type Clarification } from "../../api";
import { isAdmin, onNotice, state, throttle } from "../../store";
import { unread } from "../../unread";
import { clock } from "../../util";
import ClarSubject from "../../components/ClarSubject.vue";

const { t } = useI18n();
const message = useMessage();
const list = ref<Clarification[]>([]);
const teams = ref<Account[]>([]);
const load = throttle(async () => (list.value = await api.clarifications().catch(() => [])), 500);
const off = onNotice((n) => {
  if (n.type === "clarification") load();
});
onMounted(async () => {
  unread.clar = 0;
  load();
  if (isAdmin()) teams.value = (await api.accounts().catch(() => [])).filter((a) => a.role === "contestant");
});
onBeforeUnmount(off);

const newReqs = computed(() => list.value.filter((c) => c.from_account !== null && c.answered_ms === null).sort((a, b) => a.id - b.id));
const handled = computed(() => list.value.filter((c) => c.from_account !== null && c.answered_ms !== null).sort((a, b) => b.id - a.id));
const general = computed(() => list.value.filter((c) => c.from_account === null).sort((a, b) => b.id - a.id));

// Answer a request.
const current = ref<Clarification | null>(null);
const answer = ref("");
const toAll = ref("team");
function openReq(c: Clarification) {
  current.value = c;
  answer.value = c.answer || "";
  toAll.value = c.public ? "all" : "team";
}
async function sendAnswer() {
  if (!current.value) return;
  await api.answer(current.value.id, answer.value, toAll.value === "all");
  message.success(t("clar.sent"));
  current.value = null;
  load();
}

// New clarification from the jury.
const composing = ref(false);
const to = ref("");
const subject = ref("");
const text = ref("");
const subjectOptions = computed(() => [
  { label: t("team.general"), value: "" },
  { label: t("team.technical"), value: "#technical" },
  ...(state.info?.problems ?? []).map((p) => ({ label: `${t("team.problem")} ${p.letter} - ${p.title}`, value: p.name })),
]);
const toOptions = computed(() => [
  { label: t("team.all"), value: "" },
  ...teams.value.map((a) => ({ label: `${a.display_name || a.username} (${a.username})`, value: a.username })),
]);
async function sendNew() {
  await api.message(to.value, subject.value, text.value);
  composing.value = false;
  text.value = "";
  message.success(t("clar.sent"));
  load();
}
</script>

<template>
  <div class="dj-container">
    <div class="head">
      <h1 class="title">{{ t("nav.clarifications") }}</h1>
      <span class="grow" />
      <n-button type="primary" size="small" @click="composing = true">{{ t("clar.newTitle") }}</n-button>
    </div>

    <template v-for="sec in [{ key: 'newRequests', rows: newReqs }, { key: 'handled', rows: handled }]" :key="sec.key">
      <div class="dj-section">{{ t(`clar.${sec.key}`) }}</div>
      <table class="dj sec">
        <thead>
          <tr>
            <th>ID</th>
            <th>{{ t("team.time") }}</th>
            <th>{{ t("team.from") }}</th>
            <th>{{ t("team.subject") }}</th>
            <th>{{ t("team.text") }}</th>
            <th v-if="sec.key === 'handled'">{{ t("clar.answer") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="c in sec.rows" :key="c.id" class="link" :class="{ unseen: sec.key === 'newRequests' }" @click="openReq(c)">
            <td class="tabular">{{ c.id }}</td>
            <td class="tabular">{{ clock(c.at_ms).slice(0, 5) }}</td>
            <td>{{ c.from_name }}</td>
            <td><ClarSubject :problem="c.problem" /></td>
            <td class="text">{{ c.question }}</td>
            <td v-if="sec.key === 'handled'" class="text">{{ c.public ? `[${t("team.all")}] ` : "" }}{{ c.answer }}</td>
          </tr>
          <tr v-if="!sec.rows.length"><td colspan="6" class="muted center">{{ sec.key === "newRequests" ? t("clar.noNew") : "—" }}</td></tr>
        </tbody>
      </table>
    </template>

    <div class="dj-section">{{ t("clar.general") }}</div>
    <table class="dj sec">
      <thead>
        <tr>
          <th>ID</th>
          <th>{{ t("team.time") }}</th>
          <th>{{ t("team.to") }}</th>
          <th>{{ t("team.subject") }}</th>
          <th>{{ t("team.text") }}</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="c in general" :key="c.id">
          <td class="tabular">{{ c.id }}</td>
          <td class="tabular">{{ clock(c.at_ms).slice(0, 5) }}</td>
          <td>{{ c.to_name || t("team.all") }}</td>
          <td><ClarSubject :problem="c.problem" /></td>
          <td class="text">{{ c.answer }}</td>
        </tr>
        <tr v-if="!general.length"><td colspan="5" class="muted center">—</td></tr>
      </tbody>
    </table>

    <n-modal :show="!!current" @update:show="(v: boolean) => !v && (current = null)">
      <n-card v-if="current" :title="`${t('clar.question')} #${current.id}`" style="width: min(640px, 94vw)" :bordered="false" size="small" closable @close="current = null">
        <div class="meta"><b>{{ current.from_name }}</b> · <ClarSubject :problem="current.problem" /> · <span class="tabular">{{ clock(current.at_ms) }}</span></div>
        <div class="q">{{ current.question }}</div>
        <div class="field">
          <label>{{ t("clar.send_to") }}</label>
          <n-radio-group v-model:value="toAll">
            <n-radio value="team">{{ t("clar.replyTeam") }}</n-radio>
            <n-radio value="all">{{ t("clar.replyAll") }}</n-radio>
          </n-radio-group>
        </div>
        <n-input v-model:value="answer" type="textarea" :autosize="{ minRows: 4, maxRows: 12 }" :placeholder="t('clar.answer')" />
        <template #action>
          <div class="actions">
            <n-button @click="current = null">{{ t("common.cancel") }}</n-button>
            <n-button type="primary" :disabled="!answer.trim()" @click="sendAnswer">{{ t("clar.reply") }}</n-button>
          </div>
        </template>
      </n-card>
    </n-modal>

    <n-modal v-model:show="composing">
      <n-card :title="t('clar.newTitle')" style="width: min(600px, 94vw)" :bordered="false" size="small">
        <div class="field">
          <label>{{ t("clar.send_to") }}</label>
          <n-select v-if="teams.length" v-model:value="to" :options="toOptions" filterable />
          <n-input v-else v-model:value="to" :placeholder="t('clar.toTeam')" />
        </div>
        <div class="field">
          <label>{{ t("clar.subject") }}</label>
          <n-select v-model:value="subject" :options="subjectOptions" />
        </div>
        <n-input v-model:value="text" type="textarea" :autosize="{ minRows: 4, maxRows: 12 }" :placeholder="t('clar.text')" />
        <template #action>
          <div class="actions">
            <n-button @click="composing = false">{{ t("common.cancel") }}</n-button>
            <n-button type="primary" :disabled="!text.trim()" @click="sendNew">{{ t("clar.send") }}</n-button>
          </div>
        </template>
      </n-card>
    </n-modal>
  </div>
</template>

<style scoped>
.head {
  display: flex;
  align-items: center;
  margin-bottom: 12px;
}
.title {
  font-weight: 500;
  font-size: 24px;
  margin: 4px 0;
}
.grow {
  flex: 1;
}
.sec {
  margin-bottom: 20px;
}
.text {
  max-width: 420px;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}
.center {
  text-align: center;
}
.meta {
  margin-bottom: 8px;
}
.q {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  background: var(--raised);
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 10px 12px;
  margin-bottom: 12px;
}
.field {
  margin-bottom: 10px;
}
label {
  display: block;
  font-weight: 600;
  font-size: 13.5px;
  margin-bottom: 5px;
}
.actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
