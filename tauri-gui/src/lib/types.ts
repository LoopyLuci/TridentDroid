export interface InstanceInfo {
  instance_id: string
  adb_host: string
  adb_port: number
  display_sock: string
  state: string
}

export interface VmConfig {
  vcpu_count: number
  memory_mib: number
  kernel_path: string
  initrd_path?: string
  cmdline: string
  sriov_vf?: string
  system_image?: string
  vendor_image?: string
}

export interface AppSettings {
  grpc_host: string
  grpc_port: number
  kernel_path: string
  system_image_path: string
  vendor_image_path: string
  theme: string
  vcpu_default: number
  memory_default_mib: number
  use_tls: boolean
  ca_cert_path: string
  client_cert_path: string
  client_key_path: string
}

export type Theme = 'light' | 'dark' | 'system'
