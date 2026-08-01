<template>
    <section>
        <h1>Devices</h1>
        <p v-if="loadError" class="error-text">{{ loadError }}</p>
        <TransitionGroup v-else tag="ul" name="list" class="device-list" appear>
            <li v-for="d in devices" :key="d.peer.id" class="device-item">
                <div class="device-main">
                    <span class="dot" :class="{ online: d.peer.online }"></span>
                    <component :is="osIcon(d.peer.os)" :size="16" class="nav-icon"/>
                    <strong>{{ d.peer.alias }}</strong>
                    <span v-if="d.peer.is_self" class="tag"><User :size="11"/> this device</span>
                    <span class="meta">{{ d.peer.os }} · {{ d.peer.tailscale_ips[0] ?? "" }}</span>
                </div>
                <div class="device-stats">
                    <span><ArrowUpRight :size="14"/> {{ d.sent_count }} ({{ formatSize(d.sent_bytes) }})</span>
                    <span><ArrowDownLeft :size="14"/> {{ d.received_count }} ({{ formatSize(d.received_bytes) }})</span>
                </div>
            </li>
        </TransitionGroup>
    </section>
</template>

<script setup>
import { ref, onMounted } from "vue";
import { Laptop, Smartphone, Server, ArrowUpRight, ArrowDownLeft, User } from "@lucide/vue";
import { api } from "../lib/api";
import { formatSize } from "../lib/format";

const devices = ref([]);
const loadError = ref("");

function osIcon(os) {
    const o = (os || "").toLowerCase();
    if (o.includes("android") || o.includes("ios")) return Smartphone;
    if (o.includes("linux") && o.includes("server")) return Server;
    return Laptop;
}

onMounted(async () => {
    try {
        const stats = await api.getDeviceStats();
        devices.value = stats.devices;
    } catch (e) {
        loadError.value = String(e);
    }
});
</script>

<style lang="scss" scoped>
.tag {
    display: inline-flex;
    align-items: center;
    gap: 3px;
}

.device-stats span {
    display: inline-flex;
    align-items: center;
    gap: 4px;
}
</style>
