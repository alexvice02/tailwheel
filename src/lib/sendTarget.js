import {ref} from "vue";

// Set by other views (e.g. the Tailnet graph's node context menu) right
// before navigating to /send, so SendView can preselect a target on mount.
// A shared ref rather than a Tauri event: navigation swaps the active
// component, so a listener registered in SendView's onMounted could race
// an event emitted just before it mounts and miss it entirely.
export const pendingSendTarget = ref(null);
