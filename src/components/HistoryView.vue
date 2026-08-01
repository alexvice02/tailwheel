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
                <tr v-for="row in rows" :key="row.key" :class="{ 'child-row': row.type === 'child' }">
                    <template v-if="row.type === 'group'">
                        <td>{{ formatDate(row.timestamp) }}</td>
                        <td class="direction">
                            <component :is="row.direction === 'sent' ? ArrowUpRight : ArrowDownLeft" :size="14"/>
                            {{ row.direction === "sent" ? "Sent" : "Received" }}
                        </td>
                        <td>
                            <button class="group-toggle" @click="toggleBatch(row.batchId)">
                                <ChevronRight :size="14" class="chevron" :class="{ expanded: row.expanded }"/>
                                <Folder :size="14"/>
                                {{ row.count }} files
                            </button>
                        </td>
                        <td>{{ row.peer }}</td>
                        <td>{{ formatSize(row.size) }}</td>
                        <td :class="row.failed ? 'status-failed' : 'status-completed'">
                            <component :is="row.failed ? CircleX : CircleCheck" :size="14" class="status-icon"/>
                            {{ row.failed ? `${row.failed} failed` : "completed" }}
                        </td>
                    </template>
                    <template v-else-if="row.type === 'child'">
                        <td class="muted-cell">{{ formatDate(row.record.timestamp) }}</td>
                        <td></td>
                        <td class="child-file">
                            <FileText :size="13"/>
                            {{ row.record.file_name }}
                        </td>
                        <td></td>
                        <td>{{ formatSize(row.record.size) }}</td>
                        <td :class="'status-' + row.record.status">
                            <component :is="statusIcon[row.record.status] ?? Clock" :size="13" class="status-icon"/>
                            {{ row.record.status }}
                        </td>
                    </template>
                    <template v-else>
                        <td>{{ formatDate(row.record.timestamp) }}</td>
                        <td class="direction">
                            <component :is="row.record.direction === 'sent' ? ArrowUpRight : ArrowDownLeft" :size="14"/>
                            {{ row.record.direction === "sent" ? "Sent" : "Received" }}
                        </td>
                        <td>{{ row.record.file_name }}</td>
                        <td>{{ row.record.peer_hostname }}</td>
                        <td>{{ formatSize(row.record.size) }}</td>
                        <td :class="'status-' + row.record.status">
                            <component :is="statusIcon[row.record.status] ?? Clock" :size="14" class="status-icon"/>
                            {{ row.record.status }}
                        </td>
                    </template>
                </tr>
            </TransitionGroup>
        </table>
    </section>
</template>

<script setup>
import {ref, computed, onMounted, onUnmounted} from "vue";
import {listen} from "@tauri-apps/api/event";
import {
    ArrowUpRight,
    ArrowDownLeft,
    CircleCheck,
    CircleX,
    Clock,
    Folder,
    ChevronRight,
    FileText,
} from "@lucide/vue";
import {api} from "../lib/api";
import {formatSize, formatDate} from "../lib/format";
import {groupConsecutive} from "../lib/group";

const history = ref([]);
const expandedBatches = ref(new Set());
const unlisteners = [];

const statusIcon = {
    completed: CircleCheck,
    rejected: CircleX,
    failed: CircleX,
    pending: Clock,
};

const rows = computed(() => {
    const groups = groupConsecutive(history.value, (r) => r.batch_id);
    const out = [];
    for (const g of groups) {
        if (g.items.length > 1) {
            const expanded = expandedBatches.value.has(g.key);
            const failed = g.items.filter((r) => r.status !== "completed").length;
            out.push({
                type: "group",
                key: "g:" + g.key,
                batchId: g.key,
                expanded,
                timestamp: g.items[0].timestamp,
                direction: g.items[0].direction,
                peer: g.items[0].peer_hostname,
                count: g.items.length,
                size: g.items.reduce((sum, r) => sum + r.size, 0),
                failed,
            });
            if (expanded) {
                for (const r of g.items) out.push({type: "child", key: r.id, record: r});
            }
        } else {
            out.push({type: "single", key: g.items[0].id, record: g.items[0]});
        }
    }
    return out;
});

function toggleBatch(id) {
    const next = new Set(expandedBatches.value);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    expandedBatches.value = next;
}

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

<style lang="scss" scoped>
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

.group-toggle {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    border: none;
    background: transparent;
    font-size: 13px;
    font-weight: 500;
    color: var(--text);
    border-radius: 6px;
}

.group-toggle:hover {
    background: var(--panel-alt);
    box-shadow: none;
}

.chevron {
    color: var(--muted);
    transition: transform 0.15s ease;
}

.chevron.expanded {
    transform: rotate(90deg);
}

.child-row {
    background: color-mix(in srgb, var(--panel-alt) 60%, transparent);
}

.child-file {
    display: flex;
    align-items: center;
    gap: 6px;
    padding-left: 26px;
    color: var(--muted);
}

.muted-cell {
    color: var(--muted);
}
</style>
