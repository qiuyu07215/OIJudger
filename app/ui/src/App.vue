<script setup lang="ts">
import { computed } from "vue";
import {
  NConfigProvider, NMessageProvider, NDialogProvider, NNotificationProvider, darkTheme, useOsTheme,
  zhCN, dateZhCN, enUS, dateEnUS,
} from "naive-ui";
import { useI18n } from "vue-i18n";
import Shell from "./Shell.vue";
import { overrides } from "./theme";

const os = useOsTheme();
const dark = computed(() => os.value === "dark");
const theme = computed(() => (dark.value ? darkTheme : null));
const themeOverrides = computed(() => overrides(dark.value));
const { locale } = useI18n();
const nLocale = computed(() => (locale.value === "en" ? enUS : zhCN));
const nDate = computed(() => (locale.value === "en" ? dateEnUS : dateZhCN));
</script>

<template>
  <n-config-provider :theme="theme" :theme-overrides="themeOverrides" :locale="nLocale" :date-locale="nDate" style="height: 100%">
    <n-message-provider>
      <n-notification-provider placement="bottom-right">
        <n-dialog-provider>
          <Shell />
        </n-dialog-provider>
      </n-notification-provider>
    </n-message-provider>
  </n-config-provider>
</template>
