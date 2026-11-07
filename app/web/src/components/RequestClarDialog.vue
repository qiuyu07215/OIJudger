<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { NModal, NCard, NButton, NSelect, NInput, useMessage } from "naive-ui";
import { useI18n } from "vue-i18n";
import { api } from "../api";
import { state } from "../store";

const props = defineProps<{ show: boolean }>();
const emit = defineEmits<{ close: []; sent: [] }>();
const { t } = useI18n();
const message = useMessage();
const subject = ref("");
const text = ref("");
const busy = ref(false);
watch(() => props.show, (s) => {
  if (s) {
    subject.value = "";
    text.value = "";
  }
});
const options = computed(() => [
  { label: t("team.general"), value: "" },
  { label: t("team.technical"), value: "#technical" },
  ...(state.info?.problems ?? []).map((p) => ({ label: `${t("team.problem")} ${p.letter} - ${p.title}`, value: p.name })),
]);
async function send() {
  busy.value = true;
  try {
    await api.ask(subject.value, text.value);
    message.success(t("clar.sent"));
    emit("sent");
    emit("close");
  } catch (e) {
    message.error(String(e));
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <n-modal :show="show" @update:show="(v: boolean) => !v && emit('close')">
    <n-card :title="t('clar.request')" style="width: min(560px, 94vw)" :bordered="false" size="small">
      <div class="field">
        <label>{{ t("clar.subject") }}</label>
        <n-select v-model:value="subject" :options="options" />
      </div>
      <div class="field">
        <label>{{ t("clar.text") }}</label>
        <n-input v-model:value="text" type="textarea" :autosize="{ minRows: 5, maxRows: 14 }" />
      </div>
      <template #action>
        <div class="actions">
          <n-button @click="emit('close')">{{ t("common.cancel") }}</n-button>
          <n-button type="primary" :loading="busy" :disabled="!text.trim()" @click="send">{{ t("clar.send") }}</n-button>
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
.actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
