<script setup>
import { ref, onMounted } from "vue";
import { api } from "../lib/api";
import { formatSize } from "../lib/format";

const devices = ref([]);
const loadError = ref("");

onMounted(async () => {
  try {
    const stats = await api.getDeviceStats();
    devices.value = stats.devices;
  } catch (e) {
    loadError.value = String(e);
  }
});
</script>

<template>
  <section>
    <h1>Devices</h1>
    <p v-if="loadError" class="error-text">{{ loadError }}</p>
    <ul v-else class="device-list">
      <li v-for="d in devices" :key="d.peer.id" class="device-item">
        <div class="device-main">
          <span class="dot" :class="{ online: d.peer.online }"></span>
          <strong>{{ d.peer.hostname }}</strong>
          <span v-if="d.peer.is_self" class="tag">this device</span>
          <span class="meta">{{ d.peer.os }} · {{ d.peer.tailscale_ips[0] ?? "" }}</span>
        </div>
        <div class="device-stats">
          <span>Sent {{ d.sent_count }} ({{ formatSize(d.sent_bytes) }})</span>
          <span>Received {{ d.received_count }} ({{ formatSize(d.received_bytes) }})</span>
        </div>
      </li>
    </ul>
  </section>
</template>
