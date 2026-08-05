import { invoke } from "@tauri-apps/api/core";

export const api = {
  getSettings: () => invoke("get_settings"),
  updateSettings: (settings) => invoke("update_settings", { settings }),
  // `fresh` bypasses the backend's short-lived cache of `tailscale` query
  // results. Pass it for explicit refresh clicks only, not for loads that
  // happen just because a view mounted.
  getDeviceStats: (fresh = false) => invoke("get_device_stats", { fresh }),
  getCpTargets: (fresh = false) => invoke("get_cp_targets", { fresh }),
  expandSendPaths: (paths) => invoke("expand_send_paths", { paths }),
  sendFile: (target, path, batchId) => invoke("send_file", { target, path, batchId }),
  listHistory: () => invoke("list_history"),
  clearHistory: () => invoke("clear_history"),
  listPending: () => invoke("list_pending"),
  pollNow: () => invoke("poll_now"),
  acceptPending: (id) => invoke("accept_pending", { id }),
  rejectPending: (id) => invoke("reject_pending", { id }),
};
