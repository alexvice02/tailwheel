<script setup>
import { ref, onMounted, onUnmounted } from "vue";
import { listen } from "@tauri-apps/api/event";
import { api } from "../lib/api";
import { formatSize, formatDate } from "../lib/format";

const history = ref([]);
const unlisteners = [];

async function refresh() {
  history.value = await api.listHistory();
}

onMounted(async () => {
  await refresh();
  unlisteners.push(await listen("history-updated", refresh));
  unlisteners.push(await listen("pending-updated", refresh));
});

onUnmounted(() => unlisteners.forEach((u) => u()));
</script>

<template>
  <section>
    <h1>Transfer history</h1>
    <p v-if="history.length === 0" class="empty">No transfers yet.</p>
    <table v-else class="history-table">
      <thead>
        <tr>
          <th>When</th>
          <th>Direction</th>
          <th>File</th>
          <th>Peer</th>
          <th>Size</th>
          <th>Status</th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="r in history" :key="r.id">
          <td>{{ formatDate(r.timestamp) }}</td>
          <td>{{ r.direction === "sent" ? "Sent" : "Received" }}</td>
          <td>{{ r.file_name }}</td>
          <td>{{ r.peer_hostname }}</td>
          <td>{{ formatSize(r.size) }}</td>
          <td :class="'status-' + r.status">{{ r.status }}</td>
        </tr>
      </tbody>
    </table>
  </section>
</template>
