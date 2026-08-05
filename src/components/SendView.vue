<template>
    <section>
        <h1>Send files</h1>

        <label class="field">
            <span>Send to</span>
            <div class="path-row">
                <select v-model="selectedTarget">
                    <option v-for="t in targets" :key="t.ip" :value="t.name">
                        {{ t.name }}{{ t.offline ? " (offline)" : "" }}
                    </option>
                </select>
                <button class="ghost" @click="loadTargets(true)" title="Refresh device list">
                    <RefreshCw :size="15"/>
                </button>
            </div>
            <p v-if="loadError" class="error-text">{{ loadError }}</p>
            <p v-else-if="targets.length === 0" class="empty">
                No other devices found on your tailnet yet.
            </p>
        </label>

        <div class="dropzone" :class="{ over: isDragOver }">
            <template v-if="droppedFiles.length === 0">
                <CloudUpload :size="30" class="drop-icon"/>
                <p>Drag and drop files here</p>
            </template>
            <TransitionGroup v-else tag="ul" name="list" class="file-list">
                <li v-for="f in droppedFiles" :key="f.path">
                    <span class="file-name"><FileText :size="14"/> {{ f.name }}</span>
                    <button class="ghost icon-only" @click="removeFile(f.path)">
                        <X :size="14"/>
                    </button>
                </li>
            </TransitionGroup>
        </div>

        <div class="browse-row">
            <button class="ghost" :disabled="expanding" @click="browseFiles">
                <FolderOpen :size="15"/>
                Browse files
            </button>
            <button class="ghost" :disabled="expanding" @click="browseFolder">
                <Folder :size="15"/>
                {{ expanding ? "Reading folder…" : "Browse folder" }}
            </button>
        </div>

        <button
            class="primary"
            :disabled="sending || !selectedTarget || droppedFiles.length === 0"
            @click="sendAll"
        >
            <SendIcon :size="15"/>
            {{ sending ? "Sending..." : "Send" }}
        </button>

        <div v-if="results.length" class="sent-panel">
            <div class="sent-panel-header">
                <SendIcon :size="14"/>
                <span>Sent files</span>
            </div>
            <TransitionGroup tag="div" name="list" class="sent-groups">
                <div v-for="batch in results" :key="batch.id" class="sent-group">
                    <template v-if="batch.files.length > 1">
                        <button class="sent-group-header" @click="batch.expanded = !batch.expanded">
                            <ChevronRight :size="14" class="chevron" :class="{ expanded: batch.expanded }"/>
                            <Folder :size="15"/>
                            <span class="group-title">{{ batch.files.length }} files to {{ batch.target }}</span>
                            <span class="group-size">{{ formatSize(batchSize(batch)) }}</span>
                            <span class="group-summary" :class="batchSummary(batch) === 'all sent' ? 'ok' : 'error'">
                {{ batchSummary(batch) }}
              </span>
                        </button>
                        <div v-if="batch.expanded" class="sent-group-body">
                            <div v-for="f in batch.files" :key="f.name" class="sent-row" :class="f.ok ? 'ok' : 'error'">
                                <component :is="f.ok ? CircleCheck : CircleX" :size="13"/>
                                <span class="file-name">{{ f.name }}</span>
                                <span class="row-detail">{{ f.ok ? formatSize(f.size) : f.error }}</span>
                            </div>
                        </div>
                    </template>
                    <div v-else class="sent-row single" :class="batch.files[0].ok ? 'ok' : 'error'">
                        <component :is="batch.files[0].ok ? CircleCheck : CircleX" :size="14"/>
                        <span class="file-name">{{ batch.files[0].name }} <span class="meta">→ {{ batch.target }}</span></span>
                        <span class="row-detail">{{
                                batch.files[0].ok ? formatSize(batch.files[0].size) : batch.files[0].error
                            }}</span>
                    </div>
                </div>
            </TransitionGroup>
        </div>
    </section>
</template>

<script setup>
import {ref, onMounted, onUnmounted} from "vue";
import {getCurrentWebview} from "@tauri-apps/api/webview";
import {open} from "@tauri-apps/plugin-dialog";
import {
    RefreshCw,
    CloudUpload,
    FolderOpen,
    Folder,
    FileText,
    X,
    Send as SendIcon,
    CircleCheck,
    CircleX,
    ChevronRight,
} from "@lucide/vue";
import {api} from "../lib/api";
import {formatSize} from "../lib/format";
import {pendingSendTarget} from "../lib/sendTarget";

const targets = ref([]);
const selectedTarget = ref("");
const droppedFiles = ref([]);
const isDragOver = ref(false);
const sending = ref(false);
const expanding = ref(false);
const results = ref([]);
const loadError = ref("");

let unlisten;

function pathBaseName(path) {
    return path.split(/[\\/]/).pop();
}

function addFiles(paths) {
    for (const p of paths) {
        if (!droppedFiles.value.some((f) => f.path === p)) {
            droppedFiles.value.push({path: p, name: pathBaseName(p)});
        }
    }
}

async function loadTargets(fresh = false) {
    loadError.value = "";
    try {
        targets.value = await api.getCpTargets(fresh);
        if (pendingSendTarget.value && targets.value.some((t) => t.name === pendingSendTarget.value)) {
            selectedTarget.value = pendingSendTarget.value;
        }
        pendingSendTarget.value = null;
        if (targets.value.length && !targets.value.some((t) => t.name === selectedTarget.value)) {
            const firstOnline = targets.value.find((t) => !t.offline) ?? targets.value[0];
            selectedTarget.value = firstOnline.name;
        }
    } catch (e) {
        loadError.value = String(e);
    }
}

function removeFile(path) {
    droppedFiles.value = droppedFiles.value.filter((f) => f.path !== path);
}

async function browseFiles() {
    const picked = await open({multiple: true, directory: false});
    if (picked) addFiles(Array.isArray(picked) ? picked : [picked]);
}

async function browseFolder() {
    const picked = await open({multiple: true, directory: true});
    if (!picked) return;
    const dirs = Array.isArray(picked) ? picked : [picked];
    expanding.value = true;
    try {
        const files = await api.expandSendPaths(dirs);
        addFiles(files);
    } catch (e) {
        loadError.value = String(e);
    } finally {
        expanding.value = false;
    }
}

function batchSize(batch) {
    return batch.files.reduce((sum, f) => sum + (f.size ?? 0), 0);
}

function batchSummary(batch) {
    const failed = batch.files.filter((f) => !f.ok).length;
    if (failed === 0) return "all sent";
    if (failed === batch.files.length) return "all failed";
    return `${failed} failed`;
}

async function sendAll() {
    if (!selectedTarget.value || droppedFiles.value.length === 0) return;
    sending.value = true;
    const batchId = crypto.randomUUID();
    const toSend = droppedFiles.value;
    droppedFiles.value = [];
    const files = [];
    for (const file of toSend) {
        try {
            const record = await api.sendFile(selectedTarget.value, file.path, batchId);
            files.push({name: file.name, ok: true, size: record.size});
        } catch (e) {
            files.push({name: file.name, ok: false, error: String(e)});
        }
    }
    results.value.unshift({id: batchId, target: selectedTarget.value, files, expanded: false});
    sending.value = false;
}

onMounted(async () => {
    await loadTargets();
    unlisten = await getCurrentWebview().onDragDropEvent((event) => {
        const {type} = event.payload;
        if (type === "enter" || type === "over") {
            // "enter" fires once with paths already known; "over" repeats on every
            // mouse move while hovering and carries position only.
            isDragOver.value = true;
        } else if (type === "drop") {
            isDragOver.value = false;
            addFiles(event.payload.paths ?? []);
        } else {
            // "leave": drag cancelled without dropping.
            isDragOver.value = false;
        }
    });
});

onUnmounted(() => unlisten?.());
</script>

<style lang="scss" scoped>
.drop-icon {
    color: var(--muted);
    margin-bottom: 6px;
}

.file-name {
    display: inline-flex;
    align-items: center;
    gap: 6px;
}

.icon-only {
    padding: 6px;
}

.browse-row {
    display: flex;
    gap: 8px;
    margin-bottom: 16px;
}

.sent-panel {
    margin-top: 24px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--panel-alt);
    overflow: hidden;
}

.sent-panel-header {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 10px 14px;
    font-size: 13px;
    font-weight: 600;
    color: var(--muted);
    border-bottom: 1px solid var(--border);
}

.sent-groups {
    padding: 6px;
}

.sent-group + .sent-group {
    margin-top: 4px;
}

.sent-group-header {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 9px 10px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: 8px;
    cursor: pointer;
    text-align: left;
    font-size: 13px;
}

.sent-group-header:hover {
    background: var(--panel);
    border-color: var(--border);
}

.chevron {
    flex-shrink: 0;
    color: var(--muted);
    transition: transform 0.15s ease;
}

.chevron.expanded {
    transform: rotate(90deg);
}

.group-title {
    flex: 1;
    font-weight: 500;
}

.group-size {
    color: var(--muted);
    font-size: 12px;
}

.group-summary {
    font-size: 12px;
    font-weight: 600;
}

.sent-group-body {
    padding: 2px 10px 6px 34px;
    display: flex;
    flex-direction: column;
    gap: 2px;
}

.sent-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    font-size: 13px;
    border-radius: 6px;
}

.sent-row.single:hover {
    background: var(--panel);
}

.sent-row .file-name {
    flex: 1;
}

.sent-row .meta {
    color: var(--muted);
    font-weight: 400;
    font-size: 12px;
}

.row-detail {
    color: var(--muted);
    font-size: 12px;
}

.sent-row.ok svg,
.group-summary.ok {
    color: var(--ok);
}

.sent-row.error svg,
.group-summary.error {
    color: var(--danger);
}

.sent-row.error .row-detail {
    color: var(--danger);
}
</style>
