<script setup>
import { ref, onMounted, onUnmounted } from "vue";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
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
} from "@lucide/vue";
import { api } from "../lib/api";

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
      droppedFiles.value.push({ path: p, name: pathBaseName(p) });
    }
  }
}

async function loadTargets() {
  loadError.value = "";
  try {
    targets.value = await api.getCpTargets();
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
  const picked = await open({ multiple: true, directory: false });
  if (picked) addFiles(Array.isArray(picked) ? picked : [picked]);
}

async function browseFolder() {
  const picked = await open({ multiple: true, directory: true });
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

async function sendAll() {
  if (!selectedTarget.value || droppedFiles.value.length === 0) return;
  sending.value = true;
  results.value = [];
  const toSend = droppedFiles.value;
  droppedFiles.value = [];
  for (const file of toSend) {
    try {
      await api.sendFile(selectedTarget.value, file.path);
      results.value.unshift({ name: file.name, ok: true });
    } catch (e) {
      results.value.unshift({ name: file.name, ok: false, error: String(e) });
    }
  }
  sending.value = false;
}

onMounted(async () => {
  await loadTargets();
  unlisten = await getCurrentWebview().onDragDropEvent((event) => {
    const { type } = event.payload;
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
        <button class="ghost" @click="loadTargets" title="Refresh device list">
          <RefreshCw :size="15" />
        </button>
      </div>
      <p v-if="loadError" class="error-text">{{ loadError }}</p>
      <p v-else-if="targets.length === 0" class="empty">
        No other devices found on your tailnet yet.
      </p>
    </label>

    <div class="dropzone" :class="{ over: isDragOver }">
      <template v-if="droppedFiles.length === 0">
        <CloudUpload :size="30" class="drop-icon" />
        <p>Drag and drop files here</p>
      </template>
      <TransitionGroup v-else tag="ul" name="list" class="file-list">
        <li v-for="f in droppedFiles" :key="f.path">
          <span class="file-name"><FileText :size="14" /> {{ f.name }}</span>
          <button class="ghost icon-only" @click="removeFile(f.path)"><X :size="14" /></button>
        </li>
      </TransitionGroup>
    </div>

    <div class="browse-row">
      <button class="ghost" :disabled="expanding" @click="browseFiles">
        <FolderOpen :size="15" /> Browse files
      </button>
      <button class="ghost" :disabled="expanding" @click="browseFolder">
        <Folder :size="15" /> {{ expanding ? "Reading folder…" : "Browse folder" }}
      </button>
    </div>

    <button
      class="primary"
      :disabled="sending || !selectedTarget || droppedFiles.length === 0"
      @click="sendAll"
    >
      <SendIcon :size="15" /> {{ sending ? "Sending..." : "Send" }}
    </button>

    <TransitionGroup tag="ul" name="list" class="results">
      <li v-for="r in results" :key="r.name + r.ok" :class="r.ok ? 'ok' : 'error'">
        <span class="file-name">
          <component :is="r.ok ? CircleCheck : CircleX" :size="14" />
          {{ r.name }}
        </span>
        <span>{{ r.ok ? "sent" : r.error }}</span>
      </li>
    </TransitionGroup>
  </section>
</template>

<style scoped>
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
</style>
