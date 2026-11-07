<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { NButton, NCheckbox } from "naive-ui";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import { api, type PrintJob } from "../api";
import { onNotice, throttle } from "../store";
import { unread } from "../unread";
import { clock } from "../util";

const { t } = useI18n();
const router = useRouter();
const list = ref<PrintJob[]>([]);
const showDone = ref(false);
const load = throttle(async () => (list.value = await api.printQueue().catch(() => [])), 500);
const off = onNotice((n) => {
  if (n.type === "print") load();
});
onMounted(() => {
  unread.prints = 0;
  load();
});
onBeforeUnmount(off);

const rows = computed(() => list.value.filter((j) => showDone.value || !j.done).sort((a, b) => Number(a.done) - Number(b.done) || a.id - b.id));
function open(j: PrintJob) {
  window.open(router.resolve(`/print-job/${j.id}`).href, "_blank");
}
async function mark(j: PrintJob, done: boolean) {
  await api.printDone(j.id, done);
  load();
}
</script>

<template>
  <div class="dj-container">
    <div class="head">
      <h1 class="title">{{ t("print.queue") }}</h1>
      <span class="grow" />
      <n-checkbox v-model:checked="showDone">{{ t("print.showDone") }}</n-checkbox>
    </div>
    <table class="dj">
      <thead>
        <tr>
          <th />
          <th>ID</th>
          <th>{{ t("balloon.time") }}</th>
          <th>{{ t("balloon.team") }}</th>
          <th>{{ t("balloon.seat") }}</th>
          <th>{{ t("print.file") }}</th>
          <th />
        </tr>
      </thead>
      <tbody>
        <tr v-for="j in rows" :key="j.id" :class="{ done: j.done }">
          <td><n-button size="tiny" type="primary" @click="open(j)">{{ t("print.printNow") }}</n-button></td>
          <td class="tabular">{{ j.id }}</td>
          <td class="tabular">{{ clock(j.at_ms).slice(0, 5) }}</td>
          <td>{{ j.display_name || j.username }}</td>
          <td><b>{{ j.seat }}</b></td>
          <td class="mono">{{ j.filename || "-" }} <span class="muted">({{ j.content.split("\n").length }})</span></td>
          <td>
            <n-button v-if="!j.done" size="tiny" @click="mark(j, true)">{{ t("print.markDone") }}</n-button>
            <n-button v-else size="tiny" quaternary @click="mark(j, false)">{{ t("print.markUndone") }}</n-button>
          </td>
        </tr>
        <tr v-if="!rows.length"><td colspan="7" class="muted" style="text-align: center">—</td></tr>
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
</style>
