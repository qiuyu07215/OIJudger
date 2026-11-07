<script setup lang="ts">
import { onMounted, ref } from "vue";
import { NForm, NFormItem, NInput, NInputNumber, NSelect, NSwitch, NButton, NSpin, useMessage } from "naive-ui";
import { useI18n } from "vue-i18n";
import { api, type OnlineConfig } from "../../api";
import { refreshInfo } from "../../store";

const { t } = useI18n();
const message = useMessage();
const cfg = ref<OnlineConfig | null>(null);
const busy = ref(false);
onMounted(async () => (cfg.value = await api.config()));

async function save() {
  if (!cfg.value) return;
  busy.value = true;
  try {
    await api.saveConfig(cfg.value);
    message.success(t("common.saved"));
    await refreshInfo();
  } catch (e) {
    message.error(String(e));
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="dj-container">
  <h1 class="title">{{ t("nav.contest") }}</h1>
  <n-spin :show="!cfg">
    <div v-if="cfg" class="card form">
      <n-form label-placement="left" label-width="auto" :show-feedback="false" class="f">
        <n-form-item :label="t('cfg.format')">
          <n-select v-model:value="cfg.format" :options="(['icpc', 'oi', 'ioi'] as const).map((f) => ({ label: t(`jury.format.${f}`), value: f }))" />
        </n-form-item>
        <n-form-item :label="t('cfg.start')"><n-input v-model:value="cfg.start" placeholder="2026-11-08 09:00" /></n-form-item>
        <n-form-item :label="t('cfg.duration')"><n-input-number v-model:value="cfg.duration_min" :min="1" /></n-form-item>
        <n-form-item :label="t('cfg.freeze')"><n-input-number v-model:value="cfg.freeze_min" :min="0" /></n-form-item>
        <template v-if="cfg.format === 'icpc'">
          <n-form-item :label="t('cfg.penalty')"><n-input-number v-model:value="cfg.penalty_min" :min="0" /></n-form-item>
          <n-form-item :label="t('cfg.cePenalty')"><n-switch v-model:value="cfg.compile_error_penalty" /></n-form-item>
        </template>
        <n-form-item v-if="cfg.format === 'ioi'" :label="t('cfg.ioiScoring')">
          <n-select v-model:value="cfg.ioi_scoring" :options="[{ label: t('cfg.maxSubtask'), value: 'max_subtask' }, { label: t('cfg.maxSubmission'), value: 'max_submission' }]" />
        </n-form-item>
        <n-form-item v-if="cfg.format === 'oi'" :label="t('cfg.oiFeedback')">
          <n-select v-model:value="cfg.oi_feedback" :options="[{ label: t('cfg.fbNone'), value: 'none' }, { label: t('cfg.fbCompile'), value: 'compile' }]" />
        </n-form-item>
        <n-form-item :label="t('cfg.maxSource')"><n-input-number v-model:value="cfg.max_source_kb" :min="1" /></n-form-item>
        <n-form-item :label="t('cfg.interval')"><n-input-number v-model:value="cfg.submit_interval_s" :min="0" /></n-form-item>
        <n-form-item :label="t('cfg.publicBoard')"><n-switch v-model:value="cfg.public_scoreboard" /></n-form-item>
        <n-form-item v-if="cfg.format === 'icpc'" :label="t('cfg.medals')">
          <div class="medals">
            <n-input-number v-model:value="cfg.medals[0]" :min="0" size="small" />
            <n-input-number v-model:value="cfg.medals[1]" :min="0" size="small" />
            <n-input-number v-model:value="cfg.medals[2]" :min="0" size="small" />
          </div>
        </n-form-item>
        <n-form-item :label="t('cfg.bind')"><n-input v-model:value="cfg.bind" /></n-form-item>
      </n-form>
      <div class="muted hint">{{ t("cfg.problemsHint") }}</div>
      <div class="actions"><n-button type="primary" :loading="busy" @click="save">{{ t("common.save") }}</n-button></div>
    </div>
  </n-spin>
  </div>
</template>

<style scoped>
.title {
  font-weight: 500;
  font-size: 24px;
  margin: 4px 0 12px;
}
.medals {
  display: flex;
  gap: 6px;
}
.medals > * {
  width: 90px;
}
.form {
  max-width: 640px;
}
.f {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.hint {
  font-size: 13px;
  margin-top: 14px;
}
.actions {
  display: flex;
  justify-content: flex-end;
  margin-top: 12px;
}
</style>
