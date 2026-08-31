import { useState, useEffect } from 'react';
import { motion } from 'framer-motion';
import { Save, Sun, Moon, Monitor, Shield, FolderOpen } from 'lucide-react';
import { api } from '../lib/api';
import { useTheme } from '../hooks/useTheme';
import { toast } from 'sonner';
import type { AppSettings } from '../lib/types';

export default function Settings() {
  const { theme, setTheme } = useTheme();
  const [settings, setSettings] = useState<AppSettings>({
    grpc_host: '127.0.0.1',
    grpc_port: 9550,
    kernel_path: '',
    system_image_path: '',
    vendor_image_path: '',
    theme: 'dark',
    vcpu_default: 4,
    memory_default_mib: 4096,
    use_tls: false,
    ca_cert_path: '',
    client_cert_path: '',
    client_key_path: '',
  });

  useEffect(() => {
    api.getSettings().then(setSettings).catch(() => {})
  }, [])

  const handleSave = async () => {
    try {
      await api.saveSettings(settings)
      toast.success('Settings saved')
    } catch (e) {
      toast.error(`Failed to save: ${e}`)
    }
  }

  return (
    <div className="p-6 max-w-3xl mx-auto">
      <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }}>
        <h1 className="text-3xl font-bold mb-2">Settings</h1>
        <p className="text-muted-foreground mb-8">Configure TridentDroid preferences</p>

        <div className="space-y-6">
          {/* Theme */}
          <div className="bg-card border rounded-xl p-4">
            <h3 className="font-medium mb-4">Theme</h3>
            <div className="flex gap-2">
              <button
                onClick={() => setTheme('light')}
                className={`flex items-center gap-2 px-4 py-2 rounded-lg border ${theme === 'light' ? 'bg-primary text-primary-foreground' : ''}`}
              >
                <Sun className="w-4 h-4" /> Light
              </button>
              <button
                onClick={() => setTheme('dark')}
                className={`flex items-center gap-2 px-4 py-2 rounded-lg border ${theme === 'dark' ? 'bg-primary text-primary-foreground' : ''}`}
              >
                <Moon className="w-4 h-4" /> Dark
              </button>
              <button
                onClick={() => setTheme('system')}
                className={`flex items-center gap-2 px-4 py-2 rounded-lg border ${theme === 'system' ? 'bg-primary text-primary-foreground' : ''}`}
              >
                <Monitor className="w-4 h-4" /> System
              </button>
            </div>
          </div>

          {/* Daemon Connection */}
          <div className="bg-card border rounded-xl p-4">
            <div className="flex items-center gap-2 mb-4">
              <Shield className="w-5 h-5" />
              <h3 className="font-medium">Daemon Connection</h3>
            </div>
            <div className="grid grid-cols-2 gap-4 mb-4">
              <div>
                <label className="text-sm text-muted-foreground">Host</label>
                <input
                  type="text"
                  value={settings.grpc_host}
                  onChange={e => setSettings(s => ({ ...s, grpc_host: e.target.value }))}
                  className="w-full px-3 py-2 bg-background border rounded-lg"
                />
              </div>
              <div>
                <label className="text-sm text-muted-foreground">Port</label>
                <input
                  type="number"
                  value={settings.grpc_port}
                  onChange={e => setSettings(s => ({ ...s, grpc_port: parseInt(e.target.value) || 9550 }))}
                  className="w-full px-3 py-2 bg-background border rounded-lg"
                />
              </div>
            </div>
            
            {/* mTLS */}
            <div className="border-t pt-4 mt-4">
              <div className="flex items-center gap-2 mb-4">
                <input
                  type="checkbox"
                  checked={settings.use_tls}
                  onChange={e => setSettings(s => ({ ...s, use_tls: e.target.checked }))}
                  className="w-4 h-4"
                />
                <label className="text-sm font-medium">Use mTLS (mutual TLS)</label>
              </div>
              
              {settings.use_tls && (
                <div className="space-y-3">
                  <div>
                    <label className="text-sm text-muted-foreground">CA Certificate</label>
                    <div className="flex gap-2">
                      <input
                        type="text"
                        value={settings.ca_cert_path}
                        onChange={e => setSettings(s => ({ ...s, ca_cert_path: e.target.value }))}
                        placeholder="/path/to/ca.crt"
                        className="flex-1 px-3 py-2 bg-background border rounded-lg font-mono text-sm"
                      />
                    </div>
                  </div>
                  <div>
                    <label className="text-sm text-muted-foreground">Client Certificate</label>
                    <input
                      type="text"
                      value={settings.client_cert_path}
                      onChange={e => setSettings(s => ({ ...s, client_cert_path: e.target.value }))}
                      placeholder="/path/to/client.crt"
                      className="w-full px-3 py-2 bg-background border rounded-lg font-mono text-sm"
                    />
                  </div>
                  <div>
                    <label className="text-sm text-muted-foreground">Client Key</label>
                    <input
                      type="text"
                      value={settings.client_key_path}
                      onChange={e => setSettings(s => ({ ...s, client_key_path: e.target.value }))}
                      placeholder="/path/to/client.key"
                      className="w-full px-3 py-2 bg-background border rounded-lg font-mono text-sm"
                    />
                  </div>
                </div>
              )}
            </div>
          </div>

          {/* Default Paths */}
          <div className="bg-card border rounded-xl p-4">
            <h3 className="font-medium mb-4">Default Paths</h3>
            <div className="space-y-3">
              <div>
                <label className="text-sm text-muted-foreground">Kernel Path</label>
                <input
                  type="text"
                  value={settings.kernel_path}
                  onChange={e => setSettings(s => ({ ...s, kernel_path: e.target.value }))}
                  className="w-full px-3 py-2 bg-background border rounded-lg font-mono text-sm"
                />
              </div>
              <div>
                <label className="text-sm text-muted-foreground">System Image</label>
                <input
                  type="text"
                  value={settings.system_image_path}
                  onChange={e => setSettings(s => ({ ...s, system_image_path: e.target.value }))}
                  className="w-full px-3 py-2 bg-background border rounded-lg font-mono text-sm"
                />
              </div>
            </div>
          </div>

          {/* VM Defaults */}
          <div className="bg-card border rounded-xl p-4">
            <h3 className="font-medium mb-4">VM Defaults</h3>
            <div className="grid grid-cols-2 gap-4">
              <div>
                <label className="text-sm text-muted-foreground">Default vCPUs</label>
                <input
                  type="number"
                  value={settings.vcpu_default}
                  onChange={e => setSettings(s => ({ ...s, vcpu_default: parseInt(e.target.value) || 4 }))}
                  className="w-full px-3 py-2 bg-background border rounded-lg"
                />
              </div>
              <div>
                <label className="text-sm text-muted-foreground">Default Memory (MiB)</label>
                <input
                  type="number"
                  value={settings.memory_default_mib}
                  onChange={e => setSettings(s => ({ ...s, memory_default_mib: parseInt(e.target.value) || 4096 }))}
                  className="w-full px-3 py-2 bg-background border rounded-lg"
                />
              </div>
            </div>
          </div>

          <button onClick={handleSave} className="w-full py-3 bg-primary text-primary-foreground rounded-xl font-medium hover:bg-primary/90">
            <Save className="w-4 h-4 inline mr-2" />
            Save Settings
          </button>
        </div>
      </motion.div>
    </div>
  )
}
