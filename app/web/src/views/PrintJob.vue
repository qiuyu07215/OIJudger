<script setup lang="ts">
// Printable view of one print job (opened in a new window; prints itself).
import { computed, onMounted, ref } from "vue";
import { useI18n } from "vue-i18n";
import { useRoute } from "vue-router";
import { api, type PrintJob } from "../api";
import { clock } from "../util";

const { t } = useI18n();
const route = useRoute();
const job = ref<PrintJob | null>(null);
const lines = computed(() => (job.value?.content ?? "").replace(/\r\n/g, "\n").split("\n"));
onMounted(async () => {
  const all = await api.printQueue();
  job.value = all.find((j) => j.id === Number(route.params.id)) ?? null;
  if (job.value) setTimeout(() => window.print(), 300);
});
</script>

<template>
  <div v-if="job" class="sheet">
    <div class="header">
      <b>{{ job.display_name || job.username }}</b>
      <span>{{ t("print.header", { user: job.username, seat: job.seat || "-", time: clock(job.at_ms) }) }}</span>
      <span class="mono">{{ job.filename }}</span>
    </div>
    <table class="code">
      <tr v-for="(l, i) in lines" :key="i">
        <td class="ln">{{ i + 1 }}</td>
        <td class="src">{{ l }}</td>
      </tr>
    </table>
  </div>
</template>

<style scoped>
.sheet {
  background: #fff;
  color: #000;
  min-height: 100vh;
  padding: 12mm;
  box-sizing: border-box;
}
.header {
  display: flex;
  gap: 16px;
  border-bottom: 1px solid #000;
  padding-bottom: 4px;
  margin-bottom: 8px;
  font-size: 12px;
}
.code {
  border-collapse: collapse;
  font-family: var(--mono);
  font-size: 11px;
  line-height: 1.35;
}
.ln {
  color: #888;
  text-align: right;
  padding-right: 8px;
  vertical-align: top;
  user-select: none;
}
.src {
  white-space: pre-wrap;
  word-break: break-all;
}
</style>
