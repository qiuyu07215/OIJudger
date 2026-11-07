<script setup lang="ts">
import { ref } from "vue";
import { NButton, NInput, NAlert } from "naive-ui";
import { useI18n } from "vue-i18n";
import { useRoute, useRouter } from "vue-router";
import { api, ApiError } from "../api";
import { connect, loadSeen, refreshInfo, state } from "../store";
import { homeFor } from "../router";

const { t } = useI18n();
const router = useRouter();
const route = useRoute();
const username = ref("");
const password = ref("");
const busy = ref(false);
const error = ref("");

async function submit() {
  if (!username.value || !password.value) return;
  busy.value = true;
  error.value = "";
  try {
    await api.login(username.value.trim(), password.value);
    await refreshInfo();
    connect();
    loadSeen();
    const next = typeof route.query.next === "string" && route.query.next.startsWith("/") ? route.query.next : homeFor();
    router.replace(next);
  } catch (e) {
    error.value =
      e instanceof ApiError && e.status === 429 ? t("login.tooMany") : e instanceof ApiError && e.status === 401 ? t("login.failed") : String(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="wrap">
    <form class="box" @submit.prevent="submit">
      <svg viewBox="0 0 64 64" width="64" height="64" aria-hidden="true" class="logo">
        <rect x="4" y="4" width="56" height="56" rx="14" fill="#D97757" />
        <path d="M18 33 l9 9 l19 -20" stroke="#FAF9F5" stroke-width="7" fill="none" stroke-linecap="round" stroke-linejoin="round" />
      </svg>
      <h1>{{ t("login.title") }}</h1>
      <div class="muted contest">{{ state.info?.title }}</div>
      <n-alert v-if="error" type="error" :show-icon="false" class="err">{{ error }}</n-alert>
      <n-input v-model:value="username" :placeholder="t('login.username')" autofocus :input-props="{ autocomplete: 'username', autocapitalize: 'off' }" class="top" />
      <n-input
        v-model:value="password"
        type="password"
        :placeholder="t('login.password')"
        :input-props="{ autocomplete: 'current-password' }"
        class="bottom"
        @keyup.enter="submit"
      />
      <n-button type="primary" block size="large" :loading="busy" attr-type="submit">{{ t("login.submit") }}</n-button>
    </form>
  </div>
</template>

<style scoped>
.wrap {
  min-height: calc(100vh - 60px);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px 16px;
  box-sizing: border-box;
}
.box {
  width: min(330px, 100%);
  text-align: center;
}
.logo {
  margin-bottom: 14px;
}
h1 {
  font-weight: 400;
  font-size: 26px;
  margin: 0 0 4px;
}
.contest {
  font-size: 13px;
  margin-bottom: 18px;
}
.err {
  margin-bottom: 10px;
  text-align: left;
}
.top {
  margin-bottom: 8px;
}
.bottom {
  margin-bottom: 14px;
}
</style>
