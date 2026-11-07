<script setup lang="ts">
// DOMjudge balloons page: one row per first solve of a team; the runner
// marks it done. "Total" shows the balloons the team already has.
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { NButton, NCheckbox } from "naive-ui";
import { useI18n } from "vue-i18n";
import { api, type Balloon } from "../api";
import { onNotice, state, throttle } from "../store";
import { unread } from "../unread";
import { clock } from "../util";

const { t } = useI18n();
const list = ref<Balloon[]>([]);
const showDone = ref(false);
const load = throttle(async () => (list.value = await api.balloons().catch(() => [])), 500);
const off = onNotice((n) => {
  if (n.type === "balloon") load();
});
onMounted(() => {
  unread.balloons = 0;
  load();
});
onBeforeUnmount(off);

const rows = computed(() => list.value.filter((b) => showDone.value || !b.done).sort((a, b) => Number(a.done) - Number(b.done) || a.id - b.id));
const colorOf = (letter: string) => state.info?.problems.find((p) => p.letter === letter)?.color ?? "";
async function mark(b: Balloon, done: boolean) {
  await api.balloonDone(b.id, done);
  load();
}
</script>

<template>
  <div class="dj-container">
    <div class="head">
      <h1 class="title">{{ t("balloon.title") }}</h1>
      <span class="grow" />
      <n-checkbox v-model:checked="showDone">{{ t("balloon.showDone") }}</n-checkbox>
    </div>
    <table class="dj">
      <thead>
        <tr>
          <th />
          <th>ID</th>
          <th>{{ t("balloon.time") }}</th>
          <th>{{ t("balloon.solved") }}</th>
          <th>{{ t("balloon.team") }}</th>
          <th>{{ t("balloon.seat") }}</th>
          <th>{{ t("balloon.total") }}</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="b in rows" :key="b.id" :class="{ done: b.done }">
          <td>
            <n-button v-if="!b.done" size="tiny" type="primary" @click="mark(b, true)">{{ t("balloon.done") }}</n-button>
            <n-button v-else size="tiny" quaternary @click="mark(b, false)">{{ t("balloon.undo") }}</n-button>
          </td>
          <td class="tabular">{{ b.id }}</td>
          <td class="tabular">{{ clock(b.at_ms).slice(0, 5) }}</td>
          <td class="nowrap">
            <span class="dot big" :style="{ background: b.color || 'var(--accent-soft)' }" /> <b>{{ b.letter }}</b>
            <span v-if="b.first" class="first">★ {{ t("balloon.first") }}</span>
          </td>
          <td>{{ b.display_name || b.username }}</td>
          <td><b>{{ b.seat }}</b></td>
          <td class="nowrap">
            <span v-for="l in b.has" :key="l" class="dot" :style="{ background: colorOf(l) || 'var(--accent-soft)' }" :title="l" />
          </td>
        </tr>
        <tr v-if="!rows.length"><td colspan="7" class="muted" style="text-align: center">{{ t("balloon.empty") }}</td></tr>
      </tbody>
    </table>
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
tr.done td {
  opacity: 0.55;
}
.nowrap {
  white-space: nowrap;
}
.dot {
  margin-right: 3px;
}
.dot.big {
  width: 18px;
  height: 18px;
  vertical-align: -4px;
}
.first {
  margin-left: 8px;
  color: var(--accent);
  font-size: 12.5px;
}
</style>
