<script setup>
import { ref, onMounted, onUnmounted } from "vue";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { api } from "../lib/api";

const targets = ref([]);
const selectedTarget = ref("");
const droppedFiles = ref([]);
const isDragOver = ref(false);
const sending = ref(false);
const results = ref([]);
const loadError = ref("");

let unlisten;

function pathBaseName(path) {
  return path.split(/[\\/]/).pop();
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
      for (const p of event.payload.paths ?? []) {
        if (!droppedFiles.value.some((f) => f.path === p)) {
          droppedFiles.value.push({ path: p, name: pathBaseName(p) });
        }
      }
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
        <button class="ghost" @click="loadTargets" title="Refresh device list">Refresh</button>
      </div>
      <p v-if="loadError" class="error-text">{{ loadError }}</p>
      <p v-else-if="targets.length === 0" class="empty">
        No other devices found on your tailnet yet.
      </p>
    </label>

    <div class="dropzone" :class="{ over: isDragOver }">
      <p v-if="droppedFiles.length === 0">Drag and drop files here</p>
      <ul v-else class="file-list">
        <li v-for="f in droppedFiles" :key="f.path">
          <span>{{ f.name }}</span>
          <button class="ghost" @click="removeFile(f.path)">Remove</button>
        </li>
      </ul>
    </div>

    <button
      class="primary"
      :disabled="sending || !selectedTarget || droppedFiles.length === 0"
      @click="sendAll"
    >
      {{ sending ? "Sending..." : "Send" }}
    </button>

    <ul v-if="results.length" class="results">
      <li v-for="(r, i) in results" :key="i" :class="r.ok ? 'ok' : 'error'">
        {{ r.name }} — {{ r.ok ? "sent" : r.error }}
      </li>
    </ul>
  </section>
</template>
