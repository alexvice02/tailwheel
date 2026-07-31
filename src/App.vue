<script setup>
import { ref, computed, onMounted, onUnmounted } from "vue";
import { listen } from "@tauri-apps/api/event";
import SendView from "./components/SendView.vue";
import InboxView from "./components/InboxView.vue";
import HistoryView from "./components/HistoryView.vue";
import DevicesView from "./components/DevicesView.vue";
import SettingsView from "./components/SettingsView.vue";
import { api } from "./lib/api";

const navItems = [
  { path: "/send", label: "Send", component: SendView },
  { path: "/inbox", label: "Inbox", component: InboxView },
  { path: "/history", label: "History", component: HistoryView },
  { path: "/devices", label: "Devices", component: DevicesView },
  { path: "/settings", label: "Settings", component: SettingsView },
];
const aliases = { "/": "/send" };

const current = ref("/send");
const pendingCount = ref(0);
const unlisteners = [];

const currentComponent = computed(
  () => navItems.find((n) => n.path === current.value)?.component ?? SendView,
);

async function refreshPendingCount() {
  try {
    pendingCount.value = (await api.listPending()).length;
  } catch (e) {
    console.error(e);
  }
}

onMounted(async () => {
  unlisteners.push(
    await listen("navigate", (event) => {
      current.value = aliases[event.payload] ?? event.payload;
    }),
  );
  unlisteners.push(await listen("pending-updated", refreshPendingCount));
  unlisteners.push(await listen("history-updated", refreshPendingCount));
  refreshPendingCount();
});

onUnmounted(() => unlisteners.forEach((u) => u()));
</script>

<template>
  <div class="app-shell">
    <nav class="sidebar">
      <div class="brand">Taildrop</div>
      <button
        v-for="item in navItems"
        :key="item.path"
        class="nav-item"
        :class="{ active: current === item.path }"
        @click="current = item.path"
      >
        {{ item.label }}
        <span v-if="item.path === '/inbox' && pendingCount > 0" class="badge">{{
          pendingCount
        }}</span>
      </button>
    </nav>
    <main class="content">
      <component :is="currentComponent" @pending-changed="refreshPendingCount" />
    </main>
  </div>
</template>

<style>
:root {
  color-scheme: light dark;
  --bg: #f6f6f6;
  --panel: #ffffff;
  --border: #e2e2e2;
  --text: #0f0f0f;
  --muted: #6b6b6b;
  --accent: #396cd8;
  --accent-text: #ffffff;
  --danger: #c0392b;
  --ok: #1f9d55;
  font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
  font-size: 14px;
}

@media (prefers-color-scheme: dark) {
  :root {
    --bg: #1f1f1f;
    --panel: #2a2a2a;
    --border: #3c3c3c;
    --text: #f2f2f2;
    --muted: #a0a0a0;
    --accent: #5b8bef;
    --accent-text: #0f0f0f;
  }
}

* {
  box-sizing: border-box;
}

body {
  margin: 0;
  color: var(--text);
  background: var(--bg);
}

.app-shell {
  display: flex;
  height: 100vh;
  overflow: hidden;
}

.sidebar {
  width: 170px;
  flex-shrink: 0;
  background: var(--panel);
  border-right: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  padding: 12px 8px;
}

.brand {
  font-weight: 700;
  font-size: 16px;
  padding: 8px 12px 16px;
}

.nav-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  text-align: left;
  padding: 8px 12px;
  margin-bottom: 2px;
  border: none;
  background: transparent;
  color: var(--text);
  border-radius: 6px;
  cursor: pointer;
  font-size: 14px;
}

.nav-item:hover {
  background: var(--bg);
}

.nav-item.active {
  background: var(--accent);
  color: var(--accent-text);
}

.badge {
  background: var(--danger);
  color: #fff;
  border-radius: 999px;
  font-size: 11px;
  padding: 1px 6px;
}

.content {
  flex: 1;
  overflow-y: auto;
  padding: 24px 32px;
}

h1 {
  font-size: 18px;
  margin: 0 0 16px;
}

.section-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 16px;
}

.section-header h1 {
  margin: 0;
}

.field {
  display: block;
  margin-bottom: 16px;
}

.field > span {
  display: block;
  margin-bottom: 6px;
  color: var(--muted);
  font-size: 13px;
}

.field.checkbox {
  display: flex;
  align-items: center;
  gap: 8px;
}

.field.checkbox > span {
  margin: 0;
  color: var(--text);
}

.path-row {
  display: flex;
  gap: 8px;
}

input[type="text"],
input[type="number"],
select {
  flex: 1;
  padding: 8px 10px;
  border-radius: 6px;
  border: 1px solid var(--border);
  background: var(--panel);
  color: var(--text);
  font-size: 14px;
}

button {
  border-radius: 6px;
  border: 1px solid transparent;
  padding: 8px 14px;
  font-size: 14px;
  cursor: pointer;
  background: var(--panel);
  color: var(--text);
  border-color: var(--border);
}

button.primary {
  background: var(--accent);
  color: var(--accent-text);
  border-color: var(--accent);
}

button.ghost {
  background: transparent;
}

button:disabled {
  opacity: 0.5;
  cursor: default;
}

.dropzone {
  border: 2px dashed var(--border);
  border-radius: 10px;
  padding: 32px;
  text-align: center;
  color: var(--muted);
  margin-bottom: 16px;
  transition: border-color 0.15s, background 0.15s;
}

.dropzone.over {
  border-color: var(--accent);
  background: color-mix(in srgb, var(--accent) 8%, transparent);
}

.file-list,
.results,
.pending-list,
.device-list {
  list-style: none;
  padding: 0;
  margin: 0;
  text-align: left;
}

.file-list li,
.results li {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 4px 0;
}

.results li.ok {
  color: var(--ok);
}

.results li.error {
  color: var(--danger);
}

.pending-item,
.device-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px;
  border: 1px solid var(--border);
  border-radius: 8px;
  margin-bottom: 8px;
  background: var(--panel);
}

.pending-info,
.device-main {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.device-main {
  flex-direction: row;
  align-items: center;
  gap: 8px;
}

.meta {
  color: var(--muted);
  font-size: 12px;
}

.pending-actions {
  display: flex;
  gap: 8px;
}

.device-stats {
  display: flex;
  gap: 16px;
  color: var(--muted);
  font-size: 13px;
}

.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--muted);
  display: inline-block;
}

.dot.online {
  background: var(--ok);
}

.tag {
  font-size: 11px;
  color: var(--muted);
  border: 1px solid var(--border);
  border-radius: 4px;
  padding: 1px 6px;
}

.empty,
.error-text {
  color: var(--muted);
  font-size: 13px;
}

.error-text {
  color: var(--danger);
}

.saved-hint {
  margin-left: 10px;
  color: var(--ok);
  font-size: 13px;
}

.history-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
}

.history-table th,
.history-table td {
  text-align: left;
  padding: 8px;
  border-bottom: 1px solid var(--border);
}

.status-completed {
  color: var(--ok);
}

.status-rejected,
.status-failed {
  color: var(--danger);
}
</style>
