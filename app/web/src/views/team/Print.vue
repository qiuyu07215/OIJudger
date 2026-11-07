<script setup lang="ts">
// DOMjudge "Print source": choose a file and print it. Here the job goes
// to the volunteers' print queue (printed with team name and seat).
import { onBeforeUnmount, onMounted, ref } from "vue";
import { NButton, useMessage } from "naive-ui";
import { useI18n } from "vue-i18n";
import { api, type PrintJob } from "../../api";
import { onNotice } from "../../store";
import { clock } from "../../util";

const { t } = useI18n();
const message = useMessage();
const file = ref<File | null>(null);
const content = ref("");
const busy = ref(false);
const jobs = ref<PrintJob[]>([]);
const load = async () => (jobs.value = (await api.myPrints().catch(() => [])).reverse());
const off = onNotice((n) => {
  if (n.type === "print") load();
});
onMounted(load);
onBeforeUnmount(off);

const input = ref<HTMLInputElement | null>(null);
async function pick(e: Event) {
  const f = (e.target as HTMLInputElement).files?.[0];
  if (!f) return;
  file.value = f;
  content.value = await f.text();
  (e.target as HTMLInputElement).value = "";
}
async function send() {
  if (!file.value) return;
  busy.value = true;
  try {
    await api.print(file.value.name, content.value);
    message.success(t("print.success"));
    file.value = null;
    content.value = "";
    load();
  } catch (e) {
    message.error(String(e));
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="dj-container narrow">
    <h1 class="title">{{ t("print.title") }}</h1>
    <div class="dj-card form">
      <div class="row">
        <label>{{ t("print.file") }}</label>
        <div>
          <n-button size="small" @click="input?.click()">{{ t("submit.choose") }}</n-button>
          <span v-if="file" class="mono name">{{ file.name }}</span>
          <input ref="input" type="file" hidden @change="pick" />
        </div>
      </div>
      <pre v-if="content" class="code preview">{{ content }}</pre>
      <div class="actions">
        <n-button type="primary" :loading="busy" :disabled="!file || !content.trim()" @click="send">{{ t("print.submit") }}</n-button>
      </div>
    </div>
    <div v-if="jobs.length" style="margin-top: 20px">
      <div class="dj-section">{{ t("print.mine") }}</div>
      <table class="dj">
        <tbody>
          <tr v-for="j in jobs" :key="j.id">
            <td class="tabular">{{ clock(j.at_ms).slice(0, 5) }}</td>
            <td class="mono">{{ j.filename }}</td>
            <td><span class="sol" :class="j.done ? 'correct' : 'queued'">{{ j.done ? t("print.done") : t("print.waiting") }}</span></td>
          </tr>
        </tbody>
      </table>
    </div>
  </div>
</template>

<style scoped>
.narrow {
  max-width: 820px;
  margin: 0 auto;
}
.title {
  font-weight: 500;
  font-size: 24px;
  margin: 4px 0 14px;
}
.form {
  padding: 16px;
}
.row {
  display: flex;
  gap: 16px;
  align-items: center;
}
label {
  font-weight: 600;
  width: 60px;
}
.name {
  margin-left: 10px;
}
.preview {
  margin-top: 12px;
  max-height: 300px;
}
.actions {
  margin-top: 12px;
  display: flex;
  justify-content: flex-end;
}
</style>
