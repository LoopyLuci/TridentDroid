import { invoke } from '@tauri-apps/api/core'
import type { InstanceInfo, VmConfig, AppSettings, SnapshotInfo } from '../lib/types'

export const api = {
  ping: () => invoke<string>('ping_daemon'),

  launchInstance: (config: VmConfig) =>
    invoke<InstanceInfo>('launch_instance', { config }),

  listInstances: () =>
    invoke<InstanceInfo[]>('list_instances'),

  stopInstance: (instanceId: string) =>
    invoke<boolean>('stop_instance', { instanceId }),

  getInstanceInfo: (instanceId: string) =>
    invoke<InstanceInfo | null>('get_instance_info', { instanceId }),

  forkInstance: (instanceId: string, count: number) =>
    invoke<InstanceInfo[]>('fork_instance', { instanceId, count }),

  createSnapshot: (instanceId: string, snapshotId: string | undefined, includeDisk: boolean) =>
    invoke<SnapshotInfo>('create_snapshot', { instanceId, snapshotId, includeDisk }),

  restoreSnapshot: (snapshotId: string, vcpuCount?: number, memoryMib?: number) =>
    invoke<InstanceInfo>('restore_snapshot', { snapshotId, vcpuCount, memoryMib }),

  adbShell: (instanceId: string, command: string) =>
    invoke<string>('adb_shell_command', { instanceId, command }),

  checkUpdates: () =>
    invoke<{ available: boolean; version: string; notes: string }>('check_updates'),

  getSettings: () =>
    invoke<AppSettings>('get_settings'),

  saveSettings: (settings: AppSettings) =>
    invoke<void>('save_settings', { settings }),
}
