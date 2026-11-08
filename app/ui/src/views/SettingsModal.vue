<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  NModal, NCard, NForm, NFormItem, NGrid, NFormItemGi, NInput, NInputNumber, NSwitch, NSelect, NButton, NSpace,
  NDivider, NAlert, NTag, NSpin, useMessage,
} from "naive-ui";
import { useI18n } from "vue-i18n";
import { api, type AppSettings, type CompilerInfo, type ProbeView } from "../api";
import { state, judging } from "../store";
import { setLocale } from "../i18n";

const show = defineModel<boolean>("show", { default: false });
const { t } = useI18n();
const message = useMessage();
const form = ref<AppSettings | null>(null);
const detected = ref<CompilerInfo[]>([]);
const probes = ref<ProbeView[] | null>(null);
const testing = ref(false);
const detecting = ref(false);

watch(show, async (v) => {
  if (!v) return;
  form.value = JSON.parse(JSON.stringify(await api.getSettings()));
  probes.value = null;
  await detect(false);
});

async function detect(replace: boolean) {
  detecting.value = true;
  try {
    detected.value = await api.detectCompilers();
    if (form.value && (replace || !form.value.compilers.length)) {
      form.value.compilers = detected.value.map(({ lang, path, args }) => ({ lang, path, args }));
    }
  } finally {
    detecting.value = false;
  }
}

const version = (path: string) => detected.value.find((d) => d.path === path)?.version ?? "";

async function save() {
  if (!form.value) return;
  try {
    await api.saveSettings(form.value);
    state.settings = form.value;
    setLocale(form.value.language);
    show.value = false;
    message.success(t("common.saved"));
  } catch (e) {
    message.error(t("msg.saveFailed", { e: String(e) }));
  }
}

async function selftest() {
  if (!form.value) return;
  await api.saveSettings(form.value);
  testing.value = true;
  probes.value = null;
  try {
    probes.value = await api.runSelftest();
  } catch (e) {
    message.error(String(e), { duration: 8000 });
  } finally {
    testing.value = false;
  }
}
const allPassed = computed(() => probes.value?.every((p) => p.passed));
const langOptions = [
  { label: "简体中文", value: "zh-CN" },
  { label: "English", value: "en" },
];
</script>

<template>
  <n-modal v-model:show="show">
    <n-card :title="t('settings.title')" style="width: min(860px, 96vw)" closable @close="show = false">
      <n-form v-if="form" label-placement="left" label-width="auto" size="small" :show-feedback="false">
        <n-divider title-placement="left" style="margin-top: 0">{{ t("settings.compilers") }}</n-divider>
        <n-alert v-if="!form.compilers.length" type="warning" style="margin-bottom: 8px">{{ t("settings.noCompiler") }}</n-alert>
        <div v-for="(c, i) in form.compilers" :key="i" style="display: grid; grid-template-columns: 60px 1fr 1fr 32px; gap: 8px; align-items: center; margin-bottom: 6px">
          <n-tag size="small" :bordered="false">{{ c.lang === "cpp" ? "C++" : "C" }}</n-tag>
          <n-input v-model:value="c.path" :placeholder="t('settings.path')" class="mono" />
          <n-input v-model:value="c.args" :placeholder="t('settings.args')" class="mono" />
          <n-button quaternary size="small" @click="form.compilers.splice(i, 1)">✕</n-button>
          <div />
          <div style="grid-column: 2 / span 2; font-size: 11px; opacity: 0.55; margin-top: -4px">{{ version(c.path) }}</div>
        </div>
        <n-space :size="8">
          <n-button size="small" :loading="detecting" @click="detect(true)">{{ t("settings.detect") }}</n-button>
          <n-button size="small" @click="form.compilers.push({ lang: 'cpp', path: '', args: '-O2 -std=c++17 -static -DONLINE_JUDGE' })">＋ C++</n-button>
          <n-button size="small" @click="form.compilers.push({ lang: 'c', path: '', args: '-O2 -std=c11 -static -DONLINE_JUDGE -lm' })">＋ C</n-button>
        </n-space>

        <n-divider title-placement="left">{{ t("settings.judging") }}</n-divider>
        <n-grid :cols="2" :x-gap="24" :y-gap="10">
          <n-form-item-gi :span="2" :label="t('settings.workers', { n: state.info?.default_workers ?? '?' })">
            <n-input-number v-model:value="form.workers" :min="0" :max="64" style="width: 120px" />
          </n-form-item-gi>
          <n-form-item-gi :span="2" :label="t('settings.pin')"><n-switch v-model:value="form.pin_cores" /></n-form-item-gi>
          <n-form-item-gi :span="2" :label="t('settings.overlap')"><n-switch v-model:value="form.overlap_compile" /></n-form-item-gi>
          <n-form-item-gi :span="2" :label="t('settings.cacheLimit')">
            <n-input-number v-model:value="form.cache_limit_mb" :min="0" :step="1024" style="width: 160px" />
          </n-form-item-gi>
          <n-form-item-gi :span="2" :label="t('settings.patchStack')"><n-switch v-model:value="form.patch_stack" /></n-form-item-gi>
          <n-form-item-gi :span="2" :label="t('settings.recheckTle')"><n-switch v-model:value="form.recheck_tle" /></n-form-item-gi>
          <n-form-item-gi :label="t('settings.outputLimit')"><n-input-number v-model:value="form.output_limit_mb" :min="1" /></n-form-item-gi>
          <n-form-item-gi :label="t('settings.language')"><n-select v-model:value="form.language" :options="langOptions" /></n-form-item-gi>
          <n-form-item-gi :label="t('settings.compileTime')"><n-input-number v-model:value="form.compile_time_ms" :min="1000" :step="1000" /></n-form-item-gi>
          <n-form-item-gi :label="t('settings.compileMemory')"><n-input-number v-model:value="form.compile_memory_mb" :min="256" :step="256" /></n-form-item-gi>
          <n-form-item-gi :label="t('settings.checkerTime')"><n-input-number v-model:value="form.checker_time_ms" :min="1000" :step="1000" /></n-form-item-gi>
          <n-form-item-gi :label="t('settings.checkerMemory')"><n-input-number v-model:value="form.checker_memory_mb" :min="256" :step="256" /></n-form-item-gi>
        </n-grid>

        <n-divider title-placement="left">{{ t("settings.selftest") }}</n-divider>
        <div style="font-size: 12px; opacity: 0.7; margin-bottom: 8px">{{ t("settings.selftestHint") }}</div>
        <n-button size="small" :loading="testing" :disabled="judging" @click="selftest">{{ t("settings.runSelftest") }}</n-button>
        <n-spin :show="testing">
          <div v-if="probes" style="margin-top: 10px">
            <n-alert :type="allPassed ? 'success' : 'error'" style="margin-bottom: 8px">
              {{ allPassed ? t("settings.allPassed") : t("settings.someFailed") }}
            </n-alert>
            <div v-for="p in probes" :key="p.name" style="display: flex; gap: 8px; font-size: 12px; padding: 2px 0">
              <n-tag size="tiny" :type="p.passed ? 'success' : 'error'" :bordered="false">{{ p.passed ? "PASS" : "FAIL" }}</n-tag>
              <span style="min-width: 300px">{{ p.name }}</span>
              <span class="mono" style="opacity: 0.6; overflow: hidden; text-overflow: ellipsis; white-space: nowrap">{{ p.detail }}</span>
            </div>
          </div>
        </n-spin>
        <div style="font-size: 11px; opacity: 0.5; margin-top: 12px">
          {{ t("settings.settingsFile", { p: state.info?.settings_path ?? "" }) }} · v{{ state.info?.version }}
        </div>
      </n-form>
      <template #footer>
        <n-space justify="end">
          <n-button @click="show = false">{{ t("common.cancel") }}</n-button>
          <n-button type="primary" @click="save">{{ t("common.save") }}</n-button>
        </n-space>
      </template>
    </n-card>
  </n-modal>
</template>
