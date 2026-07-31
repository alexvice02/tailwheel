<script setup>
import { ref, onMounted } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
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
  <section v-if="settings">
    <h1>Settings</h1>

    <label class="field">
      <span>Save received files to</span>
      <div class="path-row">
        <input type="text" v-model="settings.save_dir" />
        <button class="ghost" @click="pickDir">Browse…</button>
      </div>
    </label>

    <label class="field checkbox">
      <input type="checkbox" v-model="settings.auto_accept" />
      <span>Auto-accept incoming files (skip the confirmation prompt)</span>
    </label>

    <label class="field">
      <span>If a file with the same name already exists</span>
      <select v-model="settings.conflict_policy">
        <option value="rename">Keep both (rename the new file)</option>
        <option value="overwrite">Overwrite</option>
        <option value="skip">Skip</option>
      </select>
    </label>

    <label class="field">
      <span>Check for new files every (seconds)</span>
      <input type="number" min="1" v-model.number="settings.poll_interval_secs" />
    </label>

    <button class="primary" :disabled="saving" @click="save">
      {{ saving ? "Saving..." : "Save settings" }}
    </button>
    <span v-if="saved" class="saved-hint">Saved</span>
  </section>
</template>
