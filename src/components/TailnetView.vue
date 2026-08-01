<template>
    <section class="tailnet-view">
        <div class="tailnet-header">
            <div>
                <h1>Tailnet</h1>
                <p class="tailnet-hint">Drag files onto a device to send · scroll to zoom · drag empty space to pan</p>
            </div>
            <div class="tailnet-header-actions">
                <span v-if="loadError" class="error-text">{{ loadError }}</span>
                <button class="ghost icon-only" title="Reset view" @click="resetView">
                    <Maximize :size="15"/>
                </button>
                <button class="ghost icon-only" title="Refresh" :disabled="loading" @click="loadGraph">
                    <RefreshCw :size="15" :class="{ spinning: loading }"/>
                </button>
            </div>
        </div>

        <div class="graph-canvas">
            <canvas
                ref="canvasEl"
                class="graph-surface"
                @wheel.prevent="onWheel"
                @pointerdown="onCanvasPointerDown"
                @pointermove="onCanvasHoverMove"
                @contextmenu.prevent="onNodeContextMenu"
            ></canvas>

            <p v-if="!loading && nodeCount === 0 && !loadError" class="empty graph-empty">
                No devices found on your tailnet yet.
            </p>

            <div
                v-if="contextMenu"
                class="context-menu"
                :style="{ left: contextMenu.x + 'px', top: contextMenu.y + 'px' }"
                @pointerdown.stop
            >
                <div class="context-menu__header">{{ contextMenu.alias }}</div>
                <button
                    v-for="ip in contextMenu.ips"
                    :key="ip"
                    type="button"
                    class="context-menu__item"
                    @click="copyIpFromMenu(ip)"
                >
                    <Copy :size="13"/> Copy {{ ip }}
                </button>
                <div class="context-menu__separator" v-if="contextMenu.sendTargetName"></div>
                <button
                    v-if="contextMenu.sendTargetName"
                    type="button"
                    class="context-menu__item"
                    @click="sendToNodeFromMenu"
                >
                    <SendIcon :size="13"/> Send files…
                </button>
            </div>

            <div class="toast-stack" :class="{ 'toast-stack--shifted': selectedNode }">
                <TransitionGroup name="list">
                    <div v-for="t in toasts" :key="t.id" class="toast" :class="t.kind">
                        <component :is="toastIcon(t.kind)" :size="14"/>
                        {{ t.message }}
                    </div>
                </TransitionGroup>
            </div>

            <Transition name="panel">
                <aside v-if="selectedNode" class="node-panel" @pointerdown.stop>
                    <div class="node-panel__header">
                        <div class="node-panel__title">
                            <strong>{{ selectedNode.peer.alias }}</strong>
                            <span v-if="selectedNode.isSelf" class="tag"><User :size="11"/> this device</span>
                        </div>
                        <button class="icon-btn" title="Close" @click="closeSidebar">
                            <X :size="16"/>
                        </button>
                    </div>

                    <div class="node-panel__status">
                        <span class="dot" :class="{ online: selectedNode.peer.online }"></span>
                        {{ selectedNode.peer.online ? "Online" : "Offline" }}
                        <span v-if="!selectedNode.peer.online && selectedNode.peer.last_seen" class="meta">
                            · last seen {{ formatDate(selectedNode.peer.last_seen) }}
                        </span>
                    </div>

                    <div class="node-panel__section">
                        <span class="node-panel__label">Operating system</span>
                        <span>{{ selectedNode.peer.os || "Unknown" }}</span>
                    </div>

                    <div class="node-panel__section">
                        <span class="node-panel__label">Tailscale IPs</span>
                        <div class="node-panel__ip-row">
                            <button
                                v-for="ip in selectedNode.peer.tailscale_ips"
                                :key="ip"
                                type="button"
                                class="node-panel__ip"
                                :class="{ 'node-panel__ip--copied': copiedIp === ip }"
                                title="Click to copy"
                                @click="copyIp(ip)"
                            >
                                <component :is="copiedIp === ip ? Check : Copy" :size="11"/>
                                {{ ip }}
                            </button>
                        </div>
                    </div>

                    <div class="node-panel__section">
                        <span class="node-panel__label">DNS name</span>
                        <span class="node-panel__dns">{{ selectedNode.peer.dns_name || "—" }}</span>
                    </div>

                    <div class="node-panel__stats">
                        <div>
                            <ArrowUpRight :size="14"/>
                            {{ selectedNode.stat.sent_count }} sent
                            <span class="meta">({{ formatSize(selectedNode.stat.sent_bytes) }})</span>
                        </div>
                        <div>
                            <ArrowDownLeft :size="14"/>
                            {{ selectedNode.stat.received_count }} received
                            <span class="meta">({{ formatSize(selectedNode.stat.received_bytes) }})</span>
                        </div>
                    </div>

                    <div class="node-panel__history">
                        <span class="node-panel__label"><HistoryIcon :size="13"/> Transfer history</span>
                        <p v-if="loadingHistory" class="empty">Loading…</p>
                        <p v-else-if="nodeHistory.length === 0" class="empty">No transfers with this device yet.</p>
                        <ul v-else class="node-history-list">
                            <li v-for="r in nodeHistory" :key="r.id">
                                <component :is="r.direction === 'sent' ? ArrowUpRight : ArrowDownLeft" :size="13"/>
                                <span class="node-history-list__name">{{ r.file_name }}</span>
                                <span class="node-history-list__meta">{{ formatSize(r.size) }} · {{ formatDate(r.timestamp) }}</span>
                                <span :class="'status-' + r.status">{{ r.status }}</span>
                            </li>
                        </ul>
                    </div>
                </aside>
            </Transition>
        </div>
    </section>
</template>

<script setup>
import {ref, onMounted, onUnmounted} from "vue";
import {forceSimulation, forceManyBody, forceLink, forceCollide, forceX, forceY} from "d3-force";
import {getCurrentWebview} from "@tauri-apps/api/webview";
import {emit} from "@tauri-apps/api/event";
import {
    RefreshCw,
    Maximize,
    X,
    User,
    ArrowUpRight,
    ArrowDownLeft,
    CircleCheck,
    CircleX,
    Copy,
    Check,
    Send as SendIcon,
    WifiOff,
    RotateCcwClock as HistoryIcon,
} from "@lucide/vue";
import {api} from "../lib/api";
import {formatSize, formatDate} from "../lib/format";
import {pendingSendTarget} from "../lib/sendTarget";

// --- Vue-reactive state: only what the template (sidebar/toasts/header)
// actually renders. The graph itself is drawn imperatively on <canvas> so
// dragging/panning/zooming/ticking never goes through Vue's reactivity or
// vdom diffing — that overhead (and the blurry rescale you get from the
// browser compositing a CSS-scaled SVG/DOM layer) was the whole complaint.
const canvasEl = ref(null);
const loading = ref(false);
const loadError = ref("");
const toasts = ref([]);
const nodeHistory = ref([]);
const loadingHistory = ref(false);
const selectedId = ref(null);
const selectedNode = ref(null);
const nodeCount = ref(0);
const contextMenu = ref(null);
const copiedIp = ref(null);

// --- Plain (non-reactive) graph state, mutated directly by d3-force and by
// pointer handlers, read only inside draw().
const nodes = [];
let cpTargets = [];
const viewport = {x: 0, y: 0, scale: 1};
let dropTargetId = null;
const flashState = new Map();
const sendingState = new Set();

let ctx = null;
let dpr = window.devicePixelRatio || 1;
let simulation = null;
let linkForce = null;
let resizeObserver = null;
let dragUnlisten = null;
let themeMedia = null;
let spinnerRaf = null;

const colors = {
    border: "#e3e5ee",
    muted: "#6c6f7e",
    text: "#16171d",
    panel: "#ffffff",
    accent: "#4a6cf7",
    accent2: "#7c5cf0",
    accentText: "#ffffff",
    ok: "#21a366",
    danger: "#e0503f",
    bg: "#f3f4f8",
};

function refreshColors() {
    if (!canvasEl.value) return;
    const style = getComputedStyle(canvasEl.value);
    const read = (name, fallback) => style.getPropertyValue(name).trim() || fallback;
    colors.border = read("--border", colors.border);
    colors.muted = read("--muted", colors.muted);
    colors.text = read("--text", colors.text);
    colors.panel = read("--panel", colors.panel);
    colors.accent = read("--accent", colors.accent);
    colors.accent2 = read("--accent-2", colors.accent2);
    colors.accentText = read("--accent-text", colors.accentText);
    colors.ok = read("--ok", colors.ok);
    colors.danger = read("--danger", colors.danger);
    colors.bg = read("--bg", colors.bg);
    draw();
}

function nodeRadius(n) {
    return n.isSelf ? 32 : 26;
}

function ensureSimulation(width, height) {
    if (simulation) return simulation;
    linkForce = forceLink([]).id((d) => d.id).distance(150).strength(0.55);
    simulation = forceSimulation([])
        .force("charge", forceManyBody().strength(-260))
        .force("collide", forceCollide().radius((n) => nodeRadius(n) + 6).strength(0.9))
        .force("link", linkForce)
        .force("x", forceX(width / 2).strength((n) => (n.isSelf ? 0 : 0.03)))
        .force("y", forceY(height / 2).strength((n) => (n.isSelf ? 0 : 0.03)))
        .on("tick", draw);
    return simulation;
}

// Re-centers the graph when the canvas changes size (window resize, sidebar
// toggle/drag) — otherwise the self node stays pinned at whatever offset was
// current at simulation creation and the layout drifts off-center.
function updateCenterForces() {
    if (!simulation || !canvasEl.value) return;
    const width = canvasEl.value.clientWidth;
    const height = canvasEl.value.clientHeight;
    simulation.force("x").x(width / 2);
    simulation.force("y").y(height / 2);
    const self = nodes.find((n) => n.isSelf);
    if (self) {
        self.fx = width / 2;
        self.fy = height / 2;
    }
    simulation.alpha(0.3).restart();
}

function resizeCanvas() {
    if (!canvasEl.value) return;
    dpr = window.devicePixelRatio || 1;
    const w = canvasEl.value.clientWidth;
    const h = canvasEl.value.clientHeight;
    canvasEl.value.width = Math.max(1, Math.round(w * dpr));
    canvasEl.value.height = Math.max(1, Math.round(h * dpr));
}

function resetView() {
    viewport.x = 0;
    viewport.y = 0;
    viewport.scale = 1;
    draw();
}

function screenToWorld(clientX, clientY) {
    const rect = canvasEl.value.getBoundingClientRect();
    const sx = clientX - rect.left;
    const sy = clientY - rect.top;
    return {x: (sx - viewport.x) / viewport.scale, y: (sy - viewport.y) / viewport.scale};
}

function nodeAtWorld(wx, wy) {
    let best = null;
    let bestDist = Infinity;
    for (const n of nodes) {
        const r = nodeRadius(n);
        const dist = Math.hypot(wx - n.x, wy - n.y);
        if (dist <= r && dist < bestDist) {
            best = n;
            bestDist = dist;
        }
    }
    return best;
}

function onWheel(event) {
    if (!canvasEl.value) return;
    closeContextMenu();
    const rect = canvasEl.value.getBoundingClientRect();
    const cx = event.clientX - rect.left;
    const cy = event.clientY - rect.top;
    const prevScale = viewport.scale;
    const factor = Math.exp(-event.deltaY * 0.0015);
    const nextScale = Math.min(2.2, Math.max(0.35, prevScale * factor));
    viewport.x = cx - (cx - viewport.x) * (nextScale / prevScale);
    viewport.y = cy - (cy - viewport.y) * (nextScale / prevScale);
    viewport.scale = nextScale;
    draw();
}

function onCanvasPointerDown(event) {
    // Right/middle click: leave it to the contextmenu handler, don't also
    // arm a drag/pan (pointerdown fires for every button, contextmenu fires
    // right after — without this guard a right-click would both select the
    // node underneath and open the menu).
    if (event.button !== 0) return;
    closeContextMenu();
    const {x: wx, y: wy} = screenToWorld(event.clientX, event.clientY);
    const hit = nodeAtWorld(wx, wy);
    if (hit) startNodeDrag(event, hit);
    else startPan(event);
}

function onNodeContextMenu(event) {
    const {x: wx, y: wy} = screenToWorld(event.clientX, event.clientY);
    const node = nodeAtWorld(wx, wy);
    if (!node) {
        closeContextMenu();
        return;
    }

    const rect = canvasEl.value.getBoundingClientRect();
    const menuW = 210;
    const menuH = 44 + node.peer.tailscale_ips.length * 32 + (node.sendTarget ? 38 : 0);
    const x = Math.max(8, Math.min(event.clientX - rect.left, rect.width - menuW - 8));
    const y = Math.max(8, Math.min(event.clientY - rect.top, rect.height - menuH - 8));

    contextMenu.value = {
        x,
        y,
        alias: node.peer.alias,
        ips: node.peer.tailscale_ips,
        sendTargetName: node.sendTarget?.name ?? null,
    };
    window.addEventListener("pointerdown", onWindowPointerDownForMenu);
}

function onWindowPointerDownForMenu(event) {
    if (event.target?.closest?.(".context-menu")) return;
    closeContextMenu();
}

function closeContextMenu() {
    if (!contextMenu.value) return;
    contextMenu.value = null;
    window.removeEventListener("pointerdown", onWindowPointerDownForMenu);
}

async function copyToClipboard(text) {
    try {
        await navigator.clipboard.writeText(text);
        return true;
    } catch (e) {
        console.error(e);
        pushToast(`Could not copy ${text}`, "error");
        return false;
    }
}

async function copyIp(ip) {
    if (!(await copyToClipboard(ip))) return;
    copiedIp.value = ip;
    setTimeout(() => {
        if (copiedIp.value === ip) copiedIp.value = null;
    }, 1200);
}

async function copyIpFromMenu(ip) {
    if (await copyToClipboard(ip)) pushToast(`Copied ${ip}`, "ok");
    closeContextMenu();
}

async function sendToNodeFromMenu() {
    if (!contextMenu.value?.sendTargetName) return;
    pendingSendTarget.value = contextMenu.value.sendTargetName;
    closeContextMenu();
    await emit("navigate", "/send");
}

function onCanvasHoverMove(event) {
    if (!canvasEl.value) return;
    const {x: wx, y: wy} = screenToWorld(event.clientX, event.clientY);
    canvasEl.value.style.cursor = nodeAtWorld(wx, wy) ? "pointer" : "grab";
}

function startPan(event) {
    canvasEl.value.style.cursor = "grabbing";
    const startX = event.clientX;
    const startY = event.clientY;
    const startVX = viewport.x;
    const startVY = viewport.y;
    let moved = false;

    function onMove(e) {
        const dx = e.clientX - startX;
        const dy = e.clientY - startY;
        if (!moved && Math.hypot(dx, dy) > 3) moved = true;
        viewport.x = startVX + dx;
        viewport.y = startVY + dy;
        draw();
    }

    function onUp() {
        canvasEl.value.style.cursor = "grab";
        window.removeEventListener("pointermove", onMove);
        window.removeEventListener("pointerup", onUp);
        if (!moved) closeSidebar();
    }

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
}

function startNodeDrag(event, node) {
    const startX = event.clientX;
    const startY = event.clientY;
    const startNodeX = node.x;
    const startNodeY = node.y;
    let moved = false;

    simulation?.alphaTarget(0.3).restart();
    node.fx = node.x;
    node.fy = node.y;

    function onMove(e) {
        const dx = (e.clientX - startX) / viewport.scale;
        const dy = (e.clientY - startY) / viewport.scale;
        if (!moved && Math.hypot(e.clientX - startX, e.clientY - startY) > 4) moved = true;
        node.fx = startNodeX + dx;
        node.fy = startNodeY + dy;
        draw();
    }

    function onUp() {
        window.removeEventListener("pointermove", onMove);
        window.removeEventListener("pointerup", onUp);
        simulation?.alphaTarget(0);
        if (!node.isSelf) {
            node.fx = null;
            node.fy = null;
            node.vx = 0;
            node.vy = 0;
        }
        if (!moved) selectNode(node);
    }

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
}

function selectNode(node) {
    selectedId.value = node.id;
    syncSelectedSnapshot();
    loadHistoryFor(node);
    draw();
}

function closeSidebar() {
    if (selectedId.value === null) return;
    selectedId.value = null;
    selectedNode.value = null;
    draw();
}

function syncSelectedSnapshot() {
    const n = nodes.find((x) => x.id === selectedId.value);
    selectedNode.value = n ? {id: n.id, peer: n.peer, stat: n.stat, isSelf: n.isSelf} : null;
}

async function loadHistoryFor(node) {
    loadingHistory.value = true;
    try {
        const all = await api.listHistory();
        // Received records key peer_hostname off the sender's raw OS
        // hostname (from the .tdmeta sidecar); sent records key it off
        // whatever target string we sent to (the MagicDNS short name, i.e.
        // Peer.alias) — the two directions don't share a representation,
        // so match against both.
        nodeHistory.value = all
            .filter((r) => r.peer_hostname === node.peer.hostname || r.peer_hostname === node.peer.alias)
            .slice(0, 25);
    } catch (e) {
        console.error(e);
        nodeHistory.value = [];
    } finally {
        loadingHistory.value = false;
    }
}

function flashNode(id, kind) {
    flashState.set(id, kind);
    draw();
    setTimeout(() => {
        if (flashState.get(id) === kind) {
            flashState.delete(id);
            draw();
        }
    }, 1500);
}

// Drives the "sending" spinner ring drawn in drawNode(). Only runs while at
// least one send is in flight — d3's own tick timer covers the physics
// animation the rest of the time, so this doesn't add a permanent rAF loop.
function ensureSpinnerLoop() {
    if (spinnerRaf !== null) return;
    const loop = () => {
        if (sendingState.size === 0) {
            spinnerRaf = null;
            return;
        }
        draw();
        spinnerRaf = requestAnimationFrame(loop);
    };
    spinnerRaf = requestAnimationFrame(loop);
}

function toastIcon(kind) {
    if (kind === "ok") return CircleCheck;
    if (kind === "warn") return WifiOff;
    return CircleX;
}

let toastSeq = 0;

function pushToast(message, kind) {
    const id = ++toastSeq;
    toasts.value.push({id, message, kind});
    setTimeout(() => {
        const i = toasts.value.findIndex((t) => t.id === id);
        if (i !== -1) toasts.value.splice(i, 1);
    }, 3200);
}

async function sendFilesToNode(node, paths) {
    if (!paths.length) return;

    sendingState.add(node.id);
    ensureSpinnerLoop();
    draw();
    // We already know from the last status poll whether this peer looks
    // offline — say so up front instead of leaving the user staring at a
    // spinner with no explanation while the backend waits out the timeout.
    if (!node.peer.online) {
        pushToast(`${node.peer.alias} looks offline — waiting for it to come online…`, "warn");
    }

    const batchId = paths.length > 1 ? crypto.randomUUID() : undefined;
    let ok = 0;
    let failed = 0;
    let timedOut = false;
    for (const p of paths) {
        try {
            await api.sendFile(node.sendTarget.name, p, batchId);
            ok++;
        } catch (e) {
            failed++;
            if (String(e).startsWith("timeout:")) timedOut = true;
            console.error(e);
        }
    }

    sendingState.delete(node.id);
    draw();

    flashNode(node.id, failed === 0 ? "ok" : "error");
    if (failed === 0) {
        pushToast(`Sent ${ok} file${ok > 1 ? "s" : ""} to ${node.peer.alias}`, "ok");
    } else if (timedOut) {
        pushToast(`${node.peer.alias} didn't come online in time — send cancelled`, "error");
    } else {
        pushToast(`${ok} sent, ${failed} failed → ${node.peer.alias}`, "error");
    }

    if (selectedId.value === node.id) loadHistoryFor(node);
    await refreshStatsOnly();
}

async function refreshStatsOnly() {
    try {
        const stats = await api.getDeviceStats();
        for (const d of stats.devices) {
            const n = nodes.find((x) => x.id === d.peer.id);
            if (n) {
                n.peer = d.peer;
                n.stat = d;
            }
        }
        syncSelectedSnapshot();
        draw();
    } catch (e) {
        console.error(e);
    }
}

function buildNodes(devices) {
    closeContextMenu();
    const width = canvasEl.value?.clientWidth || 800;
    const height = canvasEl.value?.clientHeight || 600;
    const existingById = new Map(nodes.map((n) => [n.id, n]));
    const next = [];

    for (const d of devices) {
        const peer = d.peer;
        const prev = existingById.get(peer.id);
        if (prev) {
            prev.peer = peer;
            prev.stat = d;
            next.push(prev);
            continue;
        }
        const angle = Math.random() * Math.PI * 2;
        const radius = peer.is_self ? 0 : 150 + Math.random() * 70;
        const node = {
            id: peer.id,
            peer,
            stat: d,
            isSelf: peer.is_self,
            x: width / 2 + Math.cos(angle) * radius,
            y: height / 2 + Math.sin(angle) * radius,
            vx: 0,
            vy: 0,
        };
        if (peer.is_self) {
            node.x = width / 2;
            node.y = height / 2;
            node.fx = width / 2;
            node.fy = height / 2;
        }
        next.push(node);
    }

    nodes.splice(0, nodes.length, ...next);
    nodeCount.value = nodes.length;

    for (const n of nodes) {
        n.sendTarget = n.isSelf ? null : cpTargets.find((t) => n.peer.tailscale_ips.includes(t.ip)) ?? null;
    }

    const self = nodes.find((n) => n.isSelf);
    const links = self ? nodes.filter((n) => !n.isSelf).map((n) => ({source: self.id, target: n.id})) : [];

    const sim = ensureSimulation(width, height);
    sim.nodes(nodes);
    linkForce.links(links);
    sim.alpha(0.7).restart();

    if (selectedId.value && !nodes.some((n) => n.id === selectedId.value)) {
        selectedId.value = null;
        selectedNode.value = null;
    } else {
        syncSelectedSnapshot();
    }
    draw();
}

async function loadGraph() {
    loading.value = true;
    loadError.value = "";
    try {
        const [stats, targets] = await Promise.all([api.getDeviceStats(), api.getCpTargets()]);
        cpTargets = targets;
        buildNodes(stats.devices);
    } catch (e) {
        loadError.value = String(e);
    } finally {
        loading.value = false;
    }
}

function onKeydown(e) {
    if (e.key !== "Escape") return;
    closeContextMenu();
    closeSidebar();
}

// --- drawing --------------------------------------------------------------

function drawOsGlyph(os, cx, cy, s, color) {
    const o = (os || "").toLowerCase();
    ctx.strokeStyle = color;
    ctx.fillStyle = color;
    ctx.lineWidth = 1.6 / viewport.scale;
    ctx.lineCap = "round";
    ctx.lineJoin = "round";

    if (o.includes("android") || o.includes("ios")) {
        const w = s * 0.85;
        const h = s * 1.5;
        ctx.beginPath();
        ctx.rect(cx - w / 2, cy - h / 2, w, h);
        ctx.stroke();
        ctx.beginPath();
        ctx.arc(cx, cy + h / 2 - 3, 1.2, 0, Math.PI * 2);
        ctx.fill();
    } else if (o.includes("linux") && o.includes("server")) {
        const w = s * 1.6;
        const h = s * 0.46;
        const gap = 3;
        for (const dir of [-1, 1]) {
            const ry = cy + dir * (h / 2 + gap / 2);
            ctx.beginPath();
            ctx.rect(cx - w / 2, ry - h / 2, w, h);
            ctx.stroke();
            ctx.beginPath();
            ctx.arc(cx - w / 2 + 4, ry, 1, 0, Math.PI * 2);
            ctx.fill();
        }
    } else {
        const w = s * 1.5;
        const h = s * 1.0;
        ctx.beginPath();
        ctx.rect(cx - w / 2, cy - h / 2 - 2, w, h);
        ctx.stroke();
        ctx.beginPath();
        ctx.moveTo(cx - w / 2 - 3, cy + h / 2 + 3);
        ctx.lineTo(cx + w / 2 + 3, cy + h / 2 + 3);
        ctx.stroke();
    }
}

function drawEdges() {
    const self = nodes.find((n) => n.isSelf);
    if (!self) return;
    for (const n of nodes) {
        if (n.isSelf) continue;
        const selected = selectedId.value === n.id;
        ctx.beginPath();
        ctx.moveTo(self.x, self.y);
        ctx.lineTo(n.x, n.y);
        ctx.strokeStyle = selected ? colors.accent : (n.peer.online ? colors.ok : colors.border);
        ctx.lineWidth = (selected ? 2 : 1.5) / viewport.scale;
        ctx.setLineDash(n.peer.online ? [] : [3 / viewport.scale, 4 / viewport.scale]);
        ctx.stroke();
    }
    ctx.setLineDash([]);
}

function drawNode(n) {
    const r = nodeRadius(n);
    const isSelected = selectedId.value === n.id;
    const isDropTarget = dropTargetId === n.id;
    const flash = flashState.get(n.id);
    const offline = !n.peer.online && !n.isSelf;

    ctx.globalAlpha = offline ? 0.55 : 1;

    ctx.beginPath();
    ctx.arc(n.x, n.y, r, 0, Math.PI * 2);
    if (n.isSelf) {
        const grad = ctx.createLinearGradient(n.x - r, n.y - r, n.x + r, n.y + r);
        grad.addColorStop(0, colors.accent);
        grad.addColorStop(1, colors.accent2);
        ctx.fillStyle = grad;
    } else {
        ctx.fillStyle = colors.panel;
    }
    ctx.fill();

    if (!n.isSelf) {
        ctx.beginPath();
        ctx.arc(n.x, n.y, r, 0, Math.PI * 2);
        ctx.strokeStyle = isDropTarget ? colors.ok : isSelected ? colors.accent : colors.border;
        ctx.lineWidth = (isDropTarget || isSelected ? 2.5 : 2) / viewport.scale;
        ctx.setLineDash(offline ? [2 / viewport.scale, 3 / viewport.scale] : []);
        ctx.stroke();
        ctx.setLineDash([]);
    }

    ctx.globalAlpha = 1;

    if (isSelected || isDropTarget) {
        ctx.beginPath();
        ctx.arc(n.x, n.y, r + 6 / viewport.scale, 0, Math.PI * 2);
        ctx.strokeStyle = isDropTarget ? colors.ok : colors.accent;
        ctx.globalAlpha = 0.25;
        ctx.lineWidth = 6 / viewport.scale;
        ctx.stroke();
        ctx.globalAlpha = 1;
    }

    drawOsGlyph(n.peer.os, n.x, n.y, r * 0.55, n.isSelf ? colors.accentText : colors.muted);

    const dotX = n.x + r * 0.72;
    const dotY = n.y + r * 0.72;
    ctx.beginPath();
    ctx.arc(dotX, dotY, 5, 0, Math.PI * 2);
    ctx.fillStyle = n.peer.online ? colors.ok : colors.muted;
    ctx.fill();
    ctx.lineWidth = 2 / viewport.scale;
    ctx.strokeStyle = colors.panel;
    ctx.stroke();

    if (sendingState.has(n.id)) {
        const angle = (performance.now() / 700) % (Math.PI * 2);
        ctx.beginPath();
        ctx.arc(n.x, n.y, r + 9 / viewport.scale, angle, angle + Math.PI * 1.3);
        ctx.strokeStyle = colors.accent;
        ctx.lineWidth = 3 / viewport.scale;
        ctx.lineCap = "round";
        ctx.stroke();
    }

    if (flash) {
        ctx.beginPath();
        ctx.arc(n.x, n.y, r + 9 / viewport.scale, 0, Math.PI * 2);
        ctx.strokeStyle = flash === "ok" ? colors.ok : colors.danger;
        ctx.lineWidth = 4 / viewport.scale;
        ctx.stroke();
    }

    const label = n.peer.alias;
    ctx.font = `${n.isSelf ? "600 " : ""}12px Inter, sans-serif`;
    ctx.textAlign = "center";
    ctx.textBaseline = "top";
    const ly = n.y + r + 8;
    const metrics = ctx.measureText(label);
    const padX = 4;
    ctx.fillStyle = colors.bg;
    ctx.globalAlpha = 0.75;
    ctx.fillRect(n.x - metrics.width / 2 - padX, ly - 1, metrics.width + padX * 2, 15);
    ctx.globalAlpha = 1;
    ctx.fillStyle = colors.text;
    ctx.fillText(label, n.x, ly);
}

function draw() {
    if (!ctx || !canvasEl.value) return;
    const w = canvasEl.value.clientWidth;
    const h = canvasEl.value.clientHeight;

    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);

    ctx.save();
    ctx.translate(viewport.x, viewport.y);
    ctx.scale(viewport.scale, viewport.scale);

    drawEdges();
    for (const n of nodes) drawNode(n);

    ctx.restore();
}

// --- lifecycle --------------------------------------------------------------

onMounted(async () => {
    ctx = canvasEl.value.getContext("2d");
    resizeCanvas();
    refreshColors();

    await loadGraph();
    draw();

    window.addEventListener("keydown", onKeydown);

    resizeObserver = new ResizeObserver(() => {
        resizeCanvas();
        updateCenterForces();
        draw();
    });
    resizeObserver.observe(canvasEl.value);

    themeMedia = window.matchMedia("(prefers-color-scheme: dark)");
    themeMedia.addEventListener("change", refreshColors);

    dragUnlisten = await getCurrentWebview().onDragDropEvent((event) => {
        const {type} = event.payload;
        if (type === "over") {
            const {x, y} = event.payload.position;
            const {x: wx, y: wy} = screenToWorld(x / window.devicePixelRatio, y / window.devicePixelRatio);
            const node = nodeAtWorld(wx, wy);
            dropTargetId = node && !node.isSelf && node.sendTarget ? node.id : null;
            draw();
        } else if (type === "drop") {
            const {x, y} = event.payload.position;
            const {x: wx, y: wy} = screenToWorld(x / window.devicePixelRatio, y / window.devicePixelRatio);
            const node = nodeAtWorld(wx, wy);
            dropTargetId = null;
            draw();
            if (node && !node.isSelf && node.sendTarget) {
                sendFilesToNode(node, event.payload.paths ?? []);
            }
        } else {
            dropTargetId = null;
            draw();
        }
    });
});

onUnmounted(() => {
    closeContextMenu();
    window.removeEventListener("keydown", onKeydown);
    resizeObserver?.disconnect();
    themeMedia?.removeEventListener("change", refreshColors);
    dragUnlisten?.();
    simulation?.stop();
    if (spinnerRaf !== null) cancelAnimationFrame(spinnerRaf);
});
</script>

<style lang="scss" scoped>
.tailnet-view {
    margin: -28px -36px;
    width: calc(100% + 72px);
    height: calc(100% + 56px);
    display: flex;
    flex-direction: column;
    position: relative;
    overflow: hidden;
}

.tailnet-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    padding: 22px 32px 14px;
    flex-shrink: 0;

    h1 {
        margin: 0 0 4px;
    }
}

.tailnet-hint {
    margin: 0;
    color: var(--muted);
    font-size: 12.5px;
}

.tailnet-header-actions {
    display: flex;
    align-items: center;
    gap: 6px;
}

.icon-only {
    padding: 7px;
}

.spinning {
    animation: spin 0.8s linear infinite;
}

@keyframes spin {
    to {
        transform: rotate(360deg);
    }
}

.graph-canvas {
    position: relative;
    flex: 1;
    overflow: hidden;
    background-image: radial-gradient(var(--border) 1px, transparent 1px);
    background-size: 24px 24px;
}

.graph-surface {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    display: block;
    cursor: grab;
    touch-action: none;
}

.graph-empty {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    pointer-events: none;
}

.toast-stack {
    position: absolute;
    right: 20px;
    bottom: 20px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    pointer-events: none;
    transition: right 0.2s ease;

    &--shifted {
        right: 320px;
    }
}

.toast {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 12px;
    border-radius: 8px;
    background: var(--panel);
    border: 1px solid var(--border);
    box-shadow: var(--shadow);
    font-size: 13px;

    &.ok svg {
        color: var(--ok);
    }

    &.error svg {
        color: var(--danger);
    }

    &.warn svg {
        color: var(--muted);
    }
}

.panel-enter-active,
.panel-leave-active {
    transition: transform 0.2s cubic-bezier(0.4, 0, 0.2, 1), opacity 0.2s ease;
}

.panel-enter-from,
.panel-leave-to {
    transform: translateX(16px);
    opacity: 0;
}

.context-menu {
    position: absolute;
    z-index: 20;
    min-width: 190px;
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: var(--shadow);
    padding: 6px;
    display: flex;
    flex-direction: column;
    gap: 2px;
}

.context-menu__header {
    padding: 6px 10px 4px;
    font-size: 12px;
    font-weight: 600;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
}

.context-menu__item {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 7px 10px;
    border: none;
    border-radius: 6px;
    background: transparent;
    font-size: 13px;
    text-align: left;
    color: var(--text);
    cursor: pointer;
}

.context-menu__item:hover {
    background: var(--panel-alt);
    box-shadow: none;
}

.context-menu__separator {
    height: 1px;
    background: var(--border);
    margin: 4px 2px;
}

.node-panel {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    width: 300px;
    background: var(--panel);
    border-left: 1px solid var(--border);
    box-shadow: var(--shadow);
    padding: 18px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 14px;
}

.node-panel__header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
}

.node-panel__title {
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 0;

    strong {
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }
}

.node-panel__status {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
}

.node-panel__section {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 13px;
}

.node-panel__label {
    display: flex;
    align-items: center;
    gap: 5px;
    color: var(--muted);
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.3px;
}

.node-panel__ip-row {
    display: flex;
    flex-direction: column;
    gap: 4px;
    align-items: flex-start;
}

.node-panel__ip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    width: fit-content;
    font-family: monospace;
    font-size: 11px;
    color: var(--muted);
    background: transparent;
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 2px 7px;
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease, color 0.15s ease;

    &:hover {
        background: var(--panel-alt);
        border-color: var(--accent);
        color: var(--text);
    }

    &--copied {
        border-color: var(--ok);
        color: var(--ok);
    }
}

.node-panel__dns {
    word-break: break-all;
    font-size: 12.5px;
    color: var(--muted);
}

.node-panel__stats {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 13px;
    background: var(--panel-alt);
    border-radius: 8px;
    padding: 10px 12px;

    div {
        display: flex;
        align-items: center;
        gap: 6px;
    }
}

.node-panel__history {
    display: flex;
    flex-direction: column;
    gap: 8px;
}

.node-history-list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
}

.node-history-list li {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12.5px;
    padding: 6px 8px;
    border-radius: 6px;
    background: var(--panel-alt);
}

.node-history-list__name {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
}

.node-history-list__meta {
    color: var(--muted);
    font-size: 11px;
    white-space: nowrap;
}
</style>
