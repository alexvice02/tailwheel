<template>
    <section v-if="settings" class="settings-panel">
        <h1>Settings</h1>

        <div class="field-group">
            <label class="field">
                <span><FolderOpen :size="18"/> Save received files to</span>
                <div class="path-row">
                    <input type="text" v-model="settings.save_dir"/>
                    <button class="ghost" @click="pickDir">Browse…</button>
                </div>
            </label>

            <label class="field checkbox">
                <input type="checkbox" v-model="settings.auto_accept"/>
                <span><Zap :size="13"/> Auto-accept incoming files (skip the confirmation prompt)</span>
            </label>
        </div>

        <label class="field">
            <span><FileExclamationPoint :size="18"/> If a file with the same name already exists</span>
            <select v-model="settings.conflict_policy">
                <option value="rename">Keep both (rename the new file)</option>
                <option value="overwrite">Overwrite</option>
                <option value="skip">Skip</option>
            </select>
            <component :is="ChevronDown" :size="13" />
        </label>

        <label class="field">
            <span><Timer :size="18"/> Check for new files every (seconds)</span>
            <input type="number" min="1" v-model.number="settings.poll_interval_secs"/>
        </label>

        <label class="field">
            <span><Trash2 :size="18"/> Automatically clear old transfer history</span>
            <select v-model="settings.history_retention">
                <option value="daily">Daily (keep last 24 hours)</option>
                <option value="weekly">Weekly (keep last 7 days)</option>
                <option value="monthly">Monthly (keep last 30 days)</option>
                <option value="never">Never (keep everything)</option>
            </select>
            <component :is="ChevronDown" :size="13" />
        </label>

        <div class="experimental">
            <button type="button" class="experimental__toggle" @click="showExperimental = !showExperimental">
                <ChevronRight :size="14" class="experimental__chevron" :class="{ 'experimental__chevron--expanded': showExperimental }"/>
                <FlaskConical :size="13"/>
                Experimental
            </button>
            <div class="collapse" :class="{ 'collapse--expanded': showExperimental }">
                <div class="collapse__inner">
                    <label class="field checkbox experimental__field">
                        <input type="checkbox" v-model="settings.hide_titlebar"/>
                        <span><PanelTop :size="13"/> Hide the native window title bar (for tiling WMs like Hyprland that don't draw one anyway; on other window managers this also removes the close/minimize/maximize controls)</span>
                    </label>
                </div>
            </div>
        </div>

        <div class="actions-row">
            <button class="primary" :disabled="saving" @click="save">
                <Save :size="15"/>
                {{ saving ? "Saving..." : "Save settings" }}
            </button>
            <Transition name="pop">
                <span v-if="saved" class="saved-hint"><Check :size="13"/> Saved</span>
            </Transition>
        </div>
    </section>
</template>

<script setup>
import {ref, onMounted} from "vue";
import {open} from "@tauri-apps/plugin-dialog";
import {FolderOpen, Zap, FileExclamationPoint, Timer, Save, Check, Trash2, ChevronRight, ChevronDown, FlaskConical, PanelTop} from "@lucide/vue";
import {api} from "../lib/api";

const settings = ref(null);
const saving = ref(false);
const saved = ref(false);
const showExperimental = ref(false);

onMounted(async () => {
    settings.value = await api.getSettings();
});

async function pickDir() {
    const dir = await open({directory: true, multiple: false, defaultPath: settings.value?.save_dir});
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

<style lang="scss" scoped>
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

.experimental {
    margin-bottom: 8px;

    &__toggle {
        border: none;
        background: transparent;
        padding: 4px 2px;
        color: var(--muted);
        font-size: 13px;
    }

    &__toggle:hover {
        box-shadow: none;
        color: var(--text);
    }

    &__chevron {
        transition: transform 0.15s ease;

        &--expanded {
            transform: rotate(90deg);
        }
    }

    &__field {
        margin: 10px 0 0;
    }
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
