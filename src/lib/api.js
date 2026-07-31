import { invoke } from "@tauri-apps/api/core";

export const api = {
  getSettings: () => invoke("get_settings"),
  updateSettings: (settings) => invoke("update_settings", { settings }),
  getDeviceStats: () => invoke("get_device_stats"),
  getCpTargets: () => invoke("get_cp_targets"),
  sendFile: (target, path) => invoke("send_file", { target, path }),
  listHistory: () => invoke("list_history"),
  listPending: () => invoke("list_pending"),
  pollNow: () => invoke("poll_now"),
  acceptPending: (id) => invoke("accept_pending", { id }),
  rejectPending: (id) => invoke("reject_pending", { id }),
};
