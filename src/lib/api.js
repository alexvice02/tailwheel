import { invoke } from "@tauri-apps/api/core";

export const api = {
  getSettings: () => invoke("get_settings"),
  updateSettings: (settings) => invoke("update_settings", { settings }),
  getDeviceStats: () => invoke("get_device_stats"),
  getCpTargets: () => invoke("get_cp_targets"),
  expandSendPaths: (paths) => invoke("expand_send_paths", { paths }),
  sendFile: (target, path, batchId) => invoke("send_file", { target, path, batchId }),
  listHistory: () => invoke("list_history"),
  listPending: () => invoke("list_pending"),
  pollNow: () => invoke("poll_now"),
  acceptPending: (id) => invoke("accept_pending", { id }),
  rejectPending: (id) => invoke("reject_pending", { id }),
};
