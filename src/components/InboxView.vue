<script setup>
import { ref, onMounted, onUnmounted } from "vue";
import { listen } from "@tauri-apps/api/event";
import { api } from "../lib/api";
import { formatSize } from "../lib/format";

const emit = defineEmits(["pending-changed"]);
const pending = ref([]);
const loading = ref(false);
const unlisteners = [];

async function refresh() {
  pending.value = await api.listPending();
}

async function pollNow() {
  loading.value = true;
  try {
    await api.pollNow();
    await refresh();
  } finally {
    loading.value = false;
  }
}

async function accept(id) {
  await api.acceptPending(id);
  await refresh();
  emit("pending-changed");
}

async function reject(id) {
  await api.rejectPending(id);
  await refresh();
  emit("pending-changed");
}

onMounted(async () => {
  await refresh();
  unlisteners.push(await listen("pending-updated", refresh));
});

onUnmounted(() => unlisteners.forEach((u) => u()));
</script>

<template>
  <section>
    <div class="section-header">
      <h1>Inbox</h1>
      <button class="ghost" :disabled="loading" @click="pollNow">
        {{ loading ? "Checking..." : "Check now" }}
      </button>
    </div>

    <p v-if="pending.length === 0" class="empty">No files waiting for confirmation.</p>
    <ul v-else class="pending-list">
      <li v-for="item in pending" :key="item.id" class="pending-item">
        <div class="pending-info">
          <strong>{{ item.file_name }}</strong>
          <span class="meta">
            {{ item.sender_hostname ?? "unknown sender" }} · {{ formatSize(item.size) }}
          </span>
        </div>
        <div class="pending-actions">
          <button class="primary" @click="accept(item.id)">Accept</button>
          <button class="ghost" @click="reject(item.id)">Reject</button>
        </div>
      </li>
    </ul>
  </section>
</template>
