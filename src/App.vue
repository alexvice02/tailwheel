<template>
    <div class="app-shell" :class="{ resizing }">
        <nav
            class="sidebar"
            :class="{ collapsed: sidebarCollapsed, resizing }"
            :style="{ width: sidebarCollapsed ? '0px' : sidebarWidth + 'px' }"
        >
            <div class="sidebar-inner">
                <div class="brand">
                    <span class="brand-mark"><Plane :size="18"/></span>
                    <span class="brand-name">tailwheel</span>
                    <button class="icon-btn collapse-btn" title="Hide sidebar" @click="toggleSidebar">
                        <PanelLeftClose :size="16"/>
                    </button>
                </div>

                <ul class="nav-list">
                    <li class="nav-list__item" v-for="parent in navItems">
                        <button
                            :class="{ 'nav-item': true, 'nav-item--expanded': isExpanded(parent.id) || !parent?.items?.length }"
                            @click.prevent="parent?.items?.length ? toggleExpanded(parent.id) : current = parent?.path"
                        >
                            <component :is="FolderCode" :size="17" class="nav-item__icon"></component>
                            {{ parent?.label }}
                            <component :is="ChevronDown" :size="17" class="nav-item__toggle" v-if="parent?.items?.length"></component>
                        </button>

                        <div class="collapse" :class="{ 'collapse--expanded': isExpanded(parent.id) }" v-if="parent?.items?.length">
                            <ul class="sub-nav-list collapse__inner">
                                <li class="sub-nav-list__item" v-for="subItem in parent?.items">
                                    <button :class="{ 'nav-item': true, 'nav-item--active': subItem.path === current }" @click="current = subItem.path">
                                        <component :is="subItem?.icon" :size="17" class="nav-item__icon" v-if="subItem?.icon"></component>
                                        {{ subItem?.label }}
                                        <component :is="ChevronDown" :size="17" class="nav-item__toggle" v-if="subItem?.items?.length"></component>
                                        <Transition name="pop">
                                            <span v-if="subItem.path === '/inbox' && pendingCount > 0" :key="pendingCount" class="badge">
                                                {{ pendingCount }}
                                            </span>
                                        </Transition>
                                    </button>
                                </li>
                            </ul>
                        </div>
                    </li>
                </ul>
            </div>
            <div v-if="!sidebarCollapsed" class="sidebar-resizer" @pointerdown="startResize"></div>
        </nav>

        <main class="content">
            <Transition name="rail">
                <button v-if="sidebarCollapsed" class="rail-toggle" title="Show sidebar" @click="toggleSidebar">
                    <PanelLeft :size="16"/>
                </button>
            </Transition>
            <Transition name="view" mode="out-in">
                <component :is="currentComponent" :key="current" @pending-changed="refreshPendingCount"/>
            </Transition>
        </main>
    </div>
</template>

<script setup>
import {ref, computed, onMounted, onUnmounted, watch} from "vue";
import {listen} from "@tauri-apps/api/event";
import {
    Send as SendIcon,
    Inbox as InboxIcon,
    RotateCcwClock as HistoryIcon,
    Laptop,
    Settings as SettingsIcon,
    Plane,
    PanelLeftClose,
    PanelLeft,
    ChevronDown,
    FolderCode,
    Dot
} from "@lucide/vue";
import SendView from "./components/SendView.vue";
import InboxView from "./components/InboxView.vue";
import HistoryView from "./components/HistoryView.vue";
import DevicesView from "./components/DevicesView.vue";
import SettingsView from "./components/SettingsView.vue";
import {api} from "./lib/api";

const navItems = [
    {id: 1, label: "Taildrop", icon: FolderCode, items: [
            {id: 2, path: "/send", label: "Send", component: SendView, icon: SendIcon},
            {id: 3, path: "/inbox", label: "Inbox", component: InboxView, icon: InboxIcon},
            {id: 4, path: "/history", label: "History", component: HistoryView, icon: HistoryIcon},
            {id: 5, path: "/devices", label: "Devices", component: DevicesView, icon: Laptop},
        ]},
    {id: 6, path: "/settings", label: "Settings", component: SettingsView, icon: SettingsIcon}
];
const expandedIds = ref(new Set([1]));
const aliases = {"/": "/send"};

const current = ref("/send");
const pendingCount = ref(0);
const unlisteners = [];

const currentComponent = computed(() =>
    navItems
        .flatMap((item) => item.items ?? [item])
        .find((item) => item.path === current.value)?.component ?? SendView
);

const WIDTH_KEY = "tailwheel:sidebar-width";
const COLLAPSED_KEY = "tailwheel:sidebar-collapsed";
const MIN_WIDTH = 200;
const MAX_WIDTH = 440;

const sidebarWidth = ref(Number(localStorage.getItem(WIDTH_KEY)) || 240);
const sidebarCollapsed = ref(localStorage.getItem(COLLAPSED_KEY) === "1");
const resizing = ref(false);

watch(sidebarWidth, (w) => localStorage.setItem(WIDTH_KEY, String(w)));
watch(sidebarCollapsed, (c) => localStorage.setItem(COLLAPSED_KEY, c ? "1" : "0"));

function isExpanded(id) {
    return expandedIds.value.has(id);
}

function toggleExpanded(id) {
    const next = new Set(expandedIds.value);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    expandedIds.value = next;
}

function toggleSidebar() {
    sidebarCollapsed.value = !sidebarCollapsed.value;
}

function startResize(event) {
    if (sidebarCollapsed.value) return;
    resizing.value = true;
    const startX = event.clientX;
    const startWidth = sidebarWidth.value;

    function onMove(e) {
        const next = startWidth + (e.clientX - startX);
        sidebarWidth.value = Math.min(MAX_WIDTH, Math.max(MIN_WIDTH, next));
    }

    function onUp() {
        resizing.value = false;
        window.removeEventListener("pointermove", onMove);
        window.removeEventListener("pointerup", onUp);
    }

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
}

async function refreshPendingCount() {
    try {
        pendingCount.value = (await api.listPending()).length;
    } catch (e) {
        console.error(e);
    }
}

onMounted(async () => {
    unlisteners.push(
        await listen("navigate", (event) => {
            current.value = aliases[event.payload] ?? event.payload;
        }),
    );
    unlisteners.push(await listen("pending-updated", refreshPendingCount));
    unlisteners.push(await listen("history-updated", refreshPendingCount));
    refreshPendingCount();
});

onUnmounted(() => unlisteners.forEach((u) => u()));
</script>

<style lang="scss">
:root {
    color-scheme: light dark;
    --bg: #f3f4f8;
    --panel: #ffffff;
    --panel-alt: #f8f9fc;
    --border: #e3e5ee;
    --text: #16171d;
    --muted: #6c6f7e;
    --accent: #4a6cf7;
    --accent-2: #7c5cf0;
    --accent-text: #ffffff;
    --danger: #e0503f;
    --ok: #21a366;
    --shadow: 0 1px 2px rgba(20, 22, 40, 0.04), 0 8px 24px rgba(20, 22, 40, 0.06);
    --shadow-sm: 0 1px 2px rgba(20, 22, 40, 0.06);
    --radius: 10px;
    font-family: "Inter", Avenir, Helvetica, Arial, sans-serif;
    font-size: 14px;
}

@media (prefers-color-scheme: dark) {
    :root {
        --bg: #17181f;
        --panel: #1e2029;
        --panel-alt: #23252f;
        --border: #2f3140;
        --text: #f1f2f6;
        --muted: #9497a8;
        --accent: #6e8bff;
        --accent-2: #9d7bff;
        --accent-text: #0f0f14;
        --shadow: 0 1px 2px rgba(0, 0, 0, 0.2), 0 8px 24px rgba(0, 0, 0, 0.35);
        --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.3);
    }
}

* {
    box-sizing: border-box;
}

body {
    margin: 0;
    color: var(--text);
    background: var(--bg);
}

.app-shell {
    display: flex;
    height: 100vh;
    overflow: hidden;
    position: relative;
}

.app-shell.resizing {
    user-select: none;
    cursor: col-resize;
}

.app-shell.resizing * {
    cursor: col-resize;
}

.sidebar {
    flex-shrink: 0;
    position: relative;
    background: linear-gradient(180deg, var(--panel) 0%, var(--panel-alt) 100%);
    border-right: 1px solid var(--border);
    overflow: hidden;
    transition: width 0.22s cubic-bezier(0.4, 0, 0.2, 1);
}

.sidebar.resizing {
    transition: none;
}

.sidebar.collapsed {
    border-right-color: transparent;
}

.sidebar-inner {
    display: flex;
    flex-direction: column;
    padding: 14px 10px;
    width: 100%;
    min-width: 200px;
    height: 100%;
}

.sidebar-resizer {
    position: absolute;
    top: 0;
    right: -3px;
    width: 6px;
    height: 100%;
    cursor: col-resize;
    z-index: 5;
}

.sidebar-resizer:hover,
.sidebar.resizing .sidebar-resizer {
    background: color-mix(in srgb, var(--accent) 40%, transparent);
}

.rail-toggle {
    width: 30px;
    height: 30px;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 8px;
    background: var(--panel);
    box-shadow: var(--shadow-sm);
    margin-bottom: 16px;
}

.rail-enter-active,
.rail-leave-active {
    transition: opacity 0.15s ease;
}

.rail-enter-from,
.rail-leave-to {
    opacity: 0;
}

.brand {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 700;
    font-size: 15px;
    padding: 6px 8px 18px;
    letter-spacing: 0.2px;
}

.brand-mark {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border-radius: 8px;
    background: linear-gradient(135deg, var(--accent), var(--accent-2));
    color: #fff;
    transform: rotate(-45deg);
}

.brand-name {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
}

.icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    padding: 0;
    border: none;
    background: transparent;
    color: var(--muted);
    border-radius: 6px;
    cursor: pointer;
    transition: background 0.15s ease, color 0.15s ease;
}

.icon-btn:hover {
    background: var(--border);
    color: var(--text);
}

.nav-list {
    list-style: none;
    padding: 0;
    margin: 0;

    & & {
        padding-left: 16px;
    }
}

.collapse {
    display: grid;
    grid-template-rows: 0fr;
    transition: grid-template-rows 0.22s cubic-bezier(0.4, 0, 0.2, 1);

    &--expanded {
        grid-template-rows: 1fr;
    }

    &__inner {
        overflow: hidden;
        min-height: 0;
    }
}

.sub-nav-list {
    padding-left: 16px;
    list-style: none;
}

.nav-item {
    display: flex;
    align-items: center;
    width: 100%;
    gap: 10px;
    text-align: left;
    padding: 9px 10px;
    margin-bottom: 3px;
    border: none;
    background: transparent;
    color: var(--text);
    border-radius: 8px;
    cursor: pointer;
    font-size: 14px;
    white-space: nowrap;
    transition: background 0.15s ease, color 0.15s ease, transform 0.15s ease;

    &__label {
        flex: 1;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    &__icon {
        flex-shrink: 0;
        color: var(--muted);
        transition: color 0.15s ease;
    }

    &__toggle {
        flex-shrink: 0;
        margin-left: auto;
        color: var(--muted);
        transition: color 0.15s ease, transform 0.15s ease;
        transform: rotate(-90deg);
    }

    &:not(&--active):hover {
        background: var(--border);
        transform: translateX(1px);
    }

    &--active {
        background: linear-gradient(135deg, var(--accent), var(--accent-2));
        color: var(--accent-text);
        box-shadow: var(--shadow-sm);
    }

    &--active &__icon {
        color: var(--accent-text);
    }

    &--expanded &__toggle, &--expanded &__icon {
        color: var(--text);
    }

    &--expanded &__toggle {
        transform: none;
    }
}

.badge {
    background: var(--danger);
    color: #fff;
    border-radius: 999px;
    font-size: 11px;
    line-height: 1;
    padding: 3px 6px;
    min-width: 8px;
    text-align: center;
}

.pop-enter-active {
    animation: pop 0.28s cubic-bezier(0.34, 1.56, 0.64, 1);
}

@keyframes pop {
    0% {
        transform: scale(0.4);
        opacity: 0;
    }
    100% {
        transform: scale(1);
        opacity: 1;
    }
}

.content {
    flex: 1;
    overflow-y: auto;
    padding: 28px 36px;
}

.view-enter-active,
.view-leave-active {
    transition: opacity 0.16s ease, transform 0.16s ease;
}

.view-enter-from {
    opacity: 0;
    transform: translateY(6px);
}

.view-leave-to {
    opacity: 0;
    transform: translateY(-4px);
}

h1 {
    font-size: 19px;
    margin: 0 0 16px;
    letter-spacing: -0.2px;
}

.section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 18px;
}

.section-header h1 {
    margin: 0;
}

.field {
    display: block;
    margin-bottom: 32px;
    position: relative;

    & > span {
        display: block;
        margin-bottom: 8px;
        color: var(--text);
        font-size: 15px;
    }

    &.checkbox {
        display: flex;
        align-items: center;
        gap: 8px;

        & > span {
            margin: 0;
            color: var(--text);
        }
    }
}

.field-group {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-bottom: 32px;

    & > .field {
        margin: 0;
    }
}

.path-row {
    display: flex;
    gap: 8px;
}

input[type="text"],
input[type="number"],
select {
    flex: 1;
    width: 100%;
    padding: 8px 10px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: #fff;
    color: var(--text);
    font-size: 14px;
    transition: border-color 0.15s ease, box-shadow 0.15s ease;
}

input[type="text"]:focus,
input[type="number"]:focus,
select:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 18%, transparent);
}

select {
    background: var(--panel);
    appearance: none;
    -webkit-appearance: none;

    & + svg {
        position: absolute;
        bottom: 10px;
        right: 10px;
        transition: transform 0.15s ease;
    }

    &:active + svg {
        transform: rotate(180deg) translateY(1px);
    }
}

button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border-radius: 8px;
    border: 1px solid transparent;
    padding: 8px 14px;
    font-size: 14px;
    cursor: pointer;
    background: var(--panel);
    color: var(--text);
    border-color: var(--border);
    transition: transform 0.1s ease, box-shadow 0.15s ease, background 0.15s ease, opacity 0.15s ease;
}

button:hover:not(:disabled) {
    box-shadow: var(--shadow-sm);
}

button:active:not(:disabled) {
    transform: scale(0.97);
}

button.primary {
    background: linear-gradient(135deg, var(--accent), var(--accent-2));
    color: var(--accent-text);
    border-color: transparent;
}

button.primary:hover:not(:disabled) {
    box-shadow: 0 4px 14px color-mix(in srgb, var(--accent) 45%, transparent);
}

button.ghost {
    background: transparent;
}

button:disabled {
    opacity: 0.5;
    cursor: default;
}

.dropzone {
    border: 2px dashed var(--border);
    border-radius: var(--radius);
    padding: 36px;
    text-align: center;
    color: var(--muted);
    margin-bottom: 16px;
    transition: border-color 0.18s ease, background 0.18s ease, transform 0.18s ease;
}

.dropzone.over {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 8%, transparent);
    transform: scale(1.01);
}

.file-list,
.results,
.pending-list,
.device-list {
    list-style: none;
    padding: 0;
    margin: 0;
    text-align: left;
}

.file-list li,
.results li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 0;
}

.results li.ok {
    color: var(--ok);
}

.results li.error {
    color: var(--danger);
}

.pending-item,
.device-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    margin-bottom: 8px;
    background: var(--panel);
    box-shadow: var(--shadow-sm);
    transition: box-shadow 0.15s ease, transform 0.15s ease;
}

.pending-item:hover,
.device-item:hover {
    box-shadow: var(--shadow);
    transform: translateY(-1px);
}

.pending-info,
.device-main {
    display: flex;
    flex-direction: column;
    gap: 4px;
}

.device-main {
    flex-direction: row;
    align-items: center;
    gap: 8px;
}

.meta {
    color: var(--muted);
    font-size: 12px;
}

.pending-actions {
    display: flex;
    gap: 8px;
}

.device-stats {
    display: flex;
    gap: 16px;
    color: var(--muted);
    font-size: 13px;
}

.dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--muted);
    display: inline-block;
}

.dot.online {
    background: var(--ok);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--ok) 25%, transparent);
}

.tag {
    font-size: 11px;
    color: var(--muted);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 1px 6px;
}

.empty,
.error-text {
    color: var(--muted);
    font-size: 13px;
}

.error-text {
    color: var(--danger);
}

.saved-hint {
    margin-left: 10px;
    color: var(--ok);
    font-size: 13px;
}

.list-move,
.list-enter-active,
.list-leave-active {
    transition: opacity 0.18s ease, transform 0.18s ease;
}

.list-enter-from {
    opacity: 0;
    transform: translateY(-6px);
}

.list-leave-to {
    opacity: 0;
    transform: translateX(8px);
}

.list-leave-active {
    position: absolute;
}

.history-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
}

.history-table th,
.history-table td {
    text-align: left;
    padding: 8px;
    border-bottom: 1px solid var(--border);
}

.status-completed {
    color: var(--ok);
}

.status-rejected,
.status-failed {
    color: var(--danger);
}
</style>
