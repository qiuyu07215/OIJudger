<script setup lang="ts">
import { computed } from "vue";
import { NConfigProvider, NMessageProvider, NDialogProvider, NNotificationProvider, darkTheme, useOsTheme, zhCN, dateZhCN, enUS, dateEnUS } from "naive-ui";
import { useI18n } from "vue-i18n";
import Shell from "./Shell.vue";

const os = useOsTheme();
const theme = computed(() => (os.value === "dark" ? darkTheme : null));
const { locale } = useI18n();
const nLocale = computed(() => (locale.value === "en" ? enUS : zhCN));
const nDate = computed(() => (locale.value === "en" ? dateEnUS : dateZhCN));
const overrides = {
  common: {
    primaryColor: "#2f7d4f",
    primaryColorHover: "#3a9460",
    primaryColorPressed: "#256640",
    primaryColorSuppl: "#3a9460",
    borderRadius: "6px",
  },
};
</script>

<template>
  <n-config-provider :theme="theme" :theme-overrides="overrides" :locale="nLocale" :date-locale="nDate" style="height: 100%">
    <n-message-provider>
      <n-notification-provider>
        <n-dialog-provider>
          <Shell />
        </n-dialog-provider>
      </n-notification-provider>
    </n-message-provider>
  </n-config-provider>
</template>
