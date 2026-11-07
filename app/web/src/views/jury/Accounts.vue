<script setup lang="ts">
import { computed, h, onMounted, reactive, ref } from "vue";
import {
  NButton, NDataTable, NModal, NCard, NForm, NFormItem, NInput, NSelect, NSwitch, NSpace, NPopconfirm,
  useDialog, useMessage, type DataTableColumns,
} from "naive-ui";
import { useI18n } from "vue-i18n";
import { useRouter } from "vue-router";
import { api, type Account, type Credential, type Role } from "../../api";
import { download } from "../../util";

const { t } = useI18n();
const message = useMessage();
const dialog = useDialog();
const router = useRouter();
const list = ref<Account[]>([]);
const filter = ref("");
const load = async () => (list.value = await api.accounts());
onMounted(load);

const roleOptions = computed(() =>
  (["contestant", "jury", "balloon", "admin"] as Role[]).map((r) => ({ label: t(`acc.roles.${r}`), value: r })),
);
const rows = computed(() => {
  const f = filter.value.toLowerCase();
  return list.value.filter((a) => !f || `${a.username} ${a.display_name} ${a.group_name} ${a.seat}`.toLowerCase().includes(f));
});

// ---- create / edit
const editing = ref(false);
const form = reactive<Account & { password: string }>({
  id: 0, username: "", display_name: "", group_name: "", seat: "", role: "contestant", ip_lock: "", disabled: false, password: "",
});
function openNew() {
  Object.assign(form, { id: 0, username: "", display_name: "", group_name: "", seat: "", role: "contestant", ip_lock: "", disabled: false, password: "" });
  editing.value = true;
}
function openEdit(a: Account) {
  Object.assign(form, a, { password: "" });
  editing.value = true;
}
async function save() {
  try {
    if (form.id) {
      await api.updateAccount({ ...form });
    } else {
      const c = await api.createAccount({ ...form });
      showCredentials([c]);
    }
    editing.value = false;
    load();
  } catch (e) {
    message.error(String(e));
  }
}

// ---- import
const importing = ref(false);
const csv = ref("");
const fileInput = ref<HTMLInputElement | null>(null);
async function pick(e: Event) {
  const f = (e.target as HTMLInputElement).files?.[0];
  if (f) csv.value = await f.text();
  (e.target as HTMLInputElement).value = "";
}
async function doImport() {
  try {
    const creds = await api.importAccounts(csv.value);
    importing.value = false;
    csv.value = "";
    showCredentials(creds);
    load();
  } catch (e) {
    message.error(String(e));
  }
}

// ---- new passwords (shown once)
const creds = ref<Credential[]>([]);
const showCreds = ref(false);
function showCredentials(c: Credential[]) {
  creds.value = c;
  showCreds.value = true;
}
function exportCsv() {
  const q = (s: string) => (/[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s);
  const lines = ["username,display_name,seat,password", ...creds.value.map((c) => [c.username, c.display_name, c.seat, c.password].map(q).join(","))];
  download("passwords.csv", lines.join("\r\n") + "\r\n");
}
function printSlips() {
  try {
    sessionStorage.setItem("oij.slips", JSON.stringify(creds.value));
  } catch {
    /* ignore */
  }
  window.open(router.resolve("/jury/slips").href, "_blank");
}

async function reset(a: Account) {
  dialog.warning({
    title: t("acc.reset"),
    content: t("acc.resetConfirm", { u: a.username }),
    positiveText: t("common.confirm"),
    negativeText: t("common.cancel"),
    onPositiveClick: async () => {
      const r = await api.resetPassword(a.id);
      showCredentials([{ username: a.username, display_name: a.display_name, seat: a.seat, password: r.password }]);
    },
  });
}
async function remove(a: Account) {
  try {
    await api.deleteAccount(a.id);
    load();
  } catch (e) {
    message.error(String(e));
  }
}

const columns = computed<DataTableColumns<Account>>(() => [
  { title: t("acc.username"), key: "username", sorter: "default" },
  { title: t("acc.display"), key: "display_name", ellipsis: { tooltip: true } },
  { title: t("acc.group"), key: "group_name", ellipsis: { tooltip: true } },
  { title: t("acc.seat"), key: "seat", width: 90 },
  { title: t("acc.role"), key: "role", width: 100, render: (a) => t(`acc.roles.${a.role}`) },
  { title: t("acc.disabled"), key: "disabled", width: 70, render: (a) => (a.disabled ? "✓" : "") },
  {
    title: t("common.actions"),
    key: "actions",
    width: 230,
    render: (a) =>
      h(NSpace, { size: 4 }, () => [
        h(NButton, { size: "tiny", onClick: () => openEdit(a) }, () => t("common.edit")),
        h(NButton, { size: "tiny", onClick: () => reset(a) }, () => t("acc.reset")),
        h(
          NPopconfirm,
          { onPositiveClick: () => remove(a) },
          { trigger: () => h(NButton, { size: "tiny", quaternary: true }, () => t("common.delete")), default: () => t("acc.deleteConfirm", { u: a.username }) },
        ),
      ]),
  },
]);
</script>

<template>
  <div class="dj-container">
    <h1 class="title">{{ t("nav.teams") }}</h1>
    <n-space style="margin-bottom: 10px">
      <n-input v-model:value="filter" clearable :placeholder="t('common.search')" style="width: 220px" />
      <n-button type="primary" @click="openNew">{{ t("acc.create") }}</n-button>
      <n-button @click="importing = true">{{ t("acc.import") }}</n-button>
    </n-space>
    <n-data-table :columns="columns" :data="rows" size="small" :row-key="(a: Account) => a.id" :pagination="{ pageSize: 50 }" />

    <n-modal v-model:show="editing">
      <n-card :title="form.id ? t('common.edit') : t('acc.create')" style="width: min(520px, 94vw)" :bordered="false">
        <n-form label-placement="left" label-width="auto">
          <n-form-item :label="t('acc.username')"><n-input v-model:value="form.username" /></n-form-item>
          <n-form-item :label="t('acc.display')"><n-input v-model:value="form.display_name" /></n-form-item>
          <n-form-item :label="t('acc.group')"><n-input v-model:value="form.group_name" /></n-form-item>
          <n-form-item :label="t('acc.seat')"><n-input v-model:value="form.seat" /></n-form-item>
          <n-form-item :label="t('acc.role')"><n-select v-model:value="form.role" :options="roleOptions" /></n-form-item>
          <n-form-item :label="t('acc.ipLock')"><n-input v-model:value="form.ip_lock" placeholder="192.168.1.23" /></n-form-item>
          <n-form-item v-if="!form.id" :label="t('acc.password')"><n-input v-model:value="form.password" /></n-form-item>
          <n-form-item v-if="form.id" :label="t('acc.disabled')"><n-switch v-model:value="form.disabled" /></n-form-item>
        </n-form>
        <template #action>
          <n-space justify="end">
            <n-button @click="editing = false">{{ t("common.cancel") }}</n-button>
            <n-button type="primary" :disabled="!form.username.trim()" @click="save">{{ t("common.save") }}</n-button>
          </n-space>
        </template>
      </n-card>
    </n-modal>

    <n-modal v-model:show="importing">
      <n-card :title="t('acc.import')" style="width: min(640px, 94vw)" :bordered="false">
        <div class="muted hint">{{ t("acc.importHint") }}</div>
        <n-input v-model:value="csv" type="textarea" class="mono-input" :autosize="{ minRows: 10, maxRows: 20 }" placeholder="team01,Team One,School A,A-01" />
        <input ref="fileInput" type="file" accept=".csv,.txt" hidden @change="pick" />
        <template #action>
          <n-space justify="end">
            <n-button @click="fileInput?.click()">{{ t("submit.choose") }}</n-button>
            <n-button type="primary" :disabled="!csv.trim()" @click="doImport">{{ t("acc.import") }}</n-button>
          </n-space>
        </template>
      </n-card>
    </n-modal>

    <n-modal v-model:show="showCreds">
      <n-card :title="t('acc.credentials')" style="width: min(640px, 94vw)" :bordered="false">
        <table class="creds mono">
          <tr v-for="c in creds" :key="c.username">
            <td>{{ c.username }}</td>
            <td class="muted">{{ c.display_name }}</td>
            <td><b>{{ c.password }}</b></td>
          </tr>
        </table>
        <template #action>
          <n-space justify="end">
            <n-button @click="exportCsv">{{ t("acc.exportCsv") }}</n-button>
            <n-button @click="printSlips">{{ t("acc.slips") }}</n-button>
            <n-button type="primary" @click="showCreds = false">{{ t("common.close") }}</n-button>
          </n-space>
        </template>
      </n-card>
    </n-modal>
  </div>
</template>

<style scoped>
.title {
  font-weight: 500;
  font-size: 24px;
  margin: 4px 0 12px;
}
.hint {
  font-size: 13px;
  margin-bottom: 8px;
}
.mono-input :deep(textarea) {
  font-family: var(--mono);
}
.creds {
  border-collapse: collapse;
  width: 100%;
  max-height: 50vh;
  overflow: auto;
  display: block;
}
.creds td {
  padding: 3px 16px 3px 0;
}
</style>
