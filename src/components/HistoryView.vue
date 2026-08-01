<script setup>
import { ref, onMounted, onUnmounted } from "vue";
import { listen } from "@tauri-apps/api/event";
import { ArrowUpRight, ArrowDownLeft, CircleCheck, CircleX, Clock } from "@lucide/vue";
import { api } from "../lib/api";
import { formatSize, formatDate } from "../lib/format";

const history = ref([]);
const unlisteners = [];

const statusIcon = {
  completed: CircleCheck,
  rejected: CircleX,
  failed: CircleX,
  pending: Clock,
};

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
      <TransitionGroup tag="tbody" name="list">
        <tr v-for="r in history" :key="r.id">
          <td>{{ formatDate(r.timestamp) }}</td>
          <td class="direction">
            <component :is="r.direction === 'sent' ? ArrowUpRight : ArrowDownLeft" :size="14" />
            {{ r.direction === "sent" ? "Sent" : "Received" }}
          </td>
          <td>{{ r.file_name }}</td>
          <td>{{ r.peer_hostname }}</td>
          <td>{{ formatSize(r.size) }}</td>
          <td :class="'status-' + r.status">
            <component :is="statusIcon[r.status] ?? Clock" :size="14" class="status-icon" />
            {{ r.status }}
          </td>
        </tr>
      </TransitionGroup>
    </table>
  </section>
</template>

<style scoped>
.direction,
.status-completed,
.status-rejected,
.status-failed {
  display: flex;
  align-items: center;
  gap: 5px;
}

.status-icon {
  flex-shrink: 0;
}
</style>
