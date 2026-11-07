<script setup lang="ts">
// DOMjudge submit flow: choose (or drop) a source file; the problem is
// taken from the file's base name (label or short name) and the language
// from its extension; confirm; back to the team page with a message.
import { computed, ref, watch } from "vue";
import { NModal, NCard, NButton, NSelect, NAlert, useDialog, useMessage } from "naive-ui";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import { api, ApiError } from "../api";
import { state, ui } from "../store";
import { guessLanguage, guessProblem } from "../util";

const { t, te } = useI18n();
const dialog = useDialog();
const message = useMessage();
const router = useRouter();

const file = ref<File | null>(null);
const source = ref("");
const problem = ref<string | null>(null);
const lang = ref<string | null>(null);
const error = ref("");
const busy = ref(false);
const dragging = ref(false);

watch(
  () => ui.submitOpen,
  (open) => {
    if (!open) return;
    file.value = null;
    source.value = "";
    error.value = "";
    lang.value = null;
    problem.value = ui.submitProblem;
  },
);

const problems = computed(() => state.info?.problems ?? []);
const problemOptions = computed(() => problems.value.map((p) => ({ label: `${p.letter} - ${p.title}`, value: p.name })));
const langOptions = [
  { label: "C++", value: "cpp" },
  { label: "C", value: "c" },
];
const maxKb = computed(() => state.info?.max_source_kb ?? 64);
const tooLarge = computed(() => !!file.value && file.value.size > maxKb.value * 1024);

async function take(f: File | undefined) {
  if (!f) return;
  file.value = f;
  source.value = await f.text();
  error.value = "";
  problem.value = guessProblem(f.name, problems.value) ?? problem.value;
  lang.value = guessLanguage(f.name) ?? lang.value;
}
const input = ref<HTMLInputElement | null>(null);
function onPick(e: Event) {
  void take((e.target as HTMLInputElement).files?.[0]);
  (e.target as HTMLInputElement).value = "";
}
function onDrop(e: DragEvent) {
  dragging.value = false;
  void take(e.dataTransfer?.files?.[0]);
}

function submit() {
  const p = problems.value.find((x) => x.name === problem.value);
  if (!p || !lang.value || !file.value) return;
  dialog.warning({
    title: t("submit.title"),
    content: t("submit.confirm", { p: `${p.letter} - ${p.title}`, l: lang.value === "c" ? "C" : "C++" }),
    positiveText: t("common.yes"),
    negativeText: t("common.cancel"),
    onPositiveClick: async () => {
      busy.value = true;
      try {
        await api.submit(p.name, lang.value!, source.value);
        ui.submitOpen = false;
        message.success(t("submit.success"));
        router.push("/team");
      } catch (e) {
        if (e instanceof ApiError && e.code === "submit_refused" && e.detail && typeof e.detail === "object") {
          const d = e.detail as Record<string, unknown> & { reason: string };
          error.value = te(`refused.${d.reason}`) ? t(`refused.${d.reason}`, d) : e.message;
        } else {
          error.value = e instanceof Error ? e.message : String(e);
        }
      } finally {
        busy.value = false;
      }
    },
  });
}
</script>

<template>
  <n-modal :show="ui.submitOpen" @update:show="(v: boolean) => (ui.submitOpen = v)">
    <n-card :title="t('submit.title')" style="width: min(560px, 94vw)" :bordered="false" size="small">
      <div class="field">
        <label>{{ t("submit.files") }}</label>
        <div
          class="drop"
          :class="{ dragging }"
          @dragover.prevent="dragging = true"
          @dragleave="dragging = false"
          @drop.prevent="onDrop"
          @click="input?.click()"
        >
          <template v-if="file">
            <b class="mono">{{ file.name }}</b>
            <span class="muted">{{ (file.size / 1024).toFixed(1) }} KB</span>
          </template>
          <template v-else>
            <n-button size="small">{{ t("submit.choose") }}</n-button>
            <span class="muted">{{ t("submit.drop") }}</span>
          </template>
        </div>
        <input ref="input" type="file" accept=".c,.cpp,.cc,.cxx" hidden @change="onPick" />
        <div class="muted small">{{ t("submit.oneFile") }}</div>
      </div>
      <div class="field">
        <label>{{ t("submit.problem") }}</label>
        <n-select v-model:value="problem" :options="problemOptions" :placeholder="t('submit.select')" />
      </div>
      <div class="field">
        <label>{{ t("submit.language") }}</label>
        <n-select v-model:value="lang" :options="langOptions" :placeholder="t('submit.select')" />
      </div>
      <div v-if="state.info?.format === 'oi'" class="muted small">{{ t("submit.oiNote") }}</div>
      <n-alert v-if="error" type="error" :show-icon="false" style="margin-top: 10px">{{ error }}</n-alert>
      <n-alert v-else-if="tooLarge" type="warning" :show-icon="false" style="margin-top: 10px">{{ t("submit.tooLarge", { n: maxKb }) }}</n-alert>
      <template #action>
        <div class="actions">
          <n-button @click="ui.submitOpen = false">{{ t("common.cancel") }}</n-button>
          <n-button type="success" :loading="busy" :disabled="!file || !problem || !lang || tooLarge" @click="submit">{{ t("submit.submit") }}</n-button>
        </div>
      </template>
    </n-card>
  </n-modal>
</template>

<style scoped>
.field {
  margin-bottom: 12px;
}
label {
  display: block;
  font-weight: 600;
  font-size: 13.5px;
  margin-bottom: 5px;
}
.drop {
  border: 2px dashed var(--border);
  border-radius: 6px;
  padding: 16px;
  display: flex;
  align-items: center;
  gap: 12px;
  cursor: pointer;
}
.drop.dragging,
.drop:hover {
  border-color: var(--accent);
}
.small {
  font-size: 12.5px;
  margin-top: 4px;
}
.actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
