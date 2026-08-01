<script setup>
import { ref, onMounted } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, Zap, FileExclamationPoint, Timer, Save, Check, Trash2 } from "@lucide/vue";
import { api } from "../lib/api";

const settings = ref(null);
const saving = ref(false);
const saved = ref(false);

onMounted(async () => {
  settings.value = await api.getSettings();
});

async function pickDir() {
  const dir = await open({ directory: true, multiple: false, defaultPath: settings.value?.save_dir });
  if (dir) settings.value.save_dir = dir;
}

async function save() {
  saving.value = true;
  saved.value = false;
  try {
    await api.updateSettings(settings.value);
    saved.value = true;
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <section v-if="settings" class="settings-panel">
    <h1>Settings</h1>

    <label class="field">
      <span><FolderOpen :size="13" /> Save received files to</span>
      <div class="path-row">
        <input type="text" v-model="settings.save_dir" />
        <button class="ghost" @click="pickDir">Browse…</button>
      </div>
    </label>

    <label class="field checkbox">
      <input type="checkbox" v-model="settings.auto_accept" />
      <span><Zap :size="13" /> Auto-accept incoming files (skip the confirmation prompt)</span>
    </label>

    <label class="field">
      <span><FileExclamationPoint :size="13" /> If a file with the same name already exists</span>
      <select v-model="settings.conflict_policy">
        <option value="rename">Keep both (rename the new file)</option>
        <option value="overwrite">Overwrite</option>
        <option value="skip">Skip</option>
      </select>
    </label>

    <label class="field">
      <span><Timer :size="13" /> Check for new files every (seconds)</span>
      <input type="number" min="1" v-model.number="settings.poll_interval_secs" />
    </label>

    <label class="field">
      <span><Trash2 :size="13" /> Automatically clear old transfer history</span>
      <select v-model="settings.history_retention">
        <option value="daily">Daily (keep last 24 hours)</option>
        <option value="weekly">Weekly (keep last 7 days)</option>
        <option value="monthly">Monthly (keep last 30 days)</option>
        <option value="never">Never (keep everything)</option>
      </select>
    </label>

    <div class="actions-row">
      <button class="primary" :disabled="saving" @click="save">
        <Save :size="15" /> {{ saving ? "Saving..." : "Save settings" }}
      </button>
      <Transition name="pop">
        <span v-if="saved" class="saved-hint"><Check :size="13" /> Saved</span>
      </Transition>
    </div>
  </section>
</template>

<style scoped>
.settings-panel {
  max-width: 460px;
}

.field {
  width: 100%;
}

.field > span {
  display: flex;
  align-items: center;
  gap: 5px;
}

.actions-row {
  display: flex;
  align-items: center;
  margin-top: 8px;
}

.saved-hint {
  margin-left: 10px;
  display: inline-flex;
  align-items: center;
  gap: 4px;
}
</style>
