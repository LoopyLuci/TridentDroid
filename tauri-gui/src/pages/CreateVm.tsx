import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { motion } from 'framer-motion'
import { ArrowLeft, FolderOpen, Cpu, MemoryStick, HardDrive } from 'lucide-react'
import { api } from '../lib/api'
import { toast } from 'sonner'

export default function CreateVm() {
  const navigate = useNavigate()
  const [config, setConfig] = useState({
    kernel_path: '',
    system_image: '',
    vendor_image: '',
    vcpu_count: 4,
    memory_mib: 4096,
    cmdline: 'console=ttyS0 earlyprintk=serial androidboot.hardware=trident',
  })
  const [launching, setLaunching] = useState(false)

  const handleLaunch = async () => {
    if (!config.kernel_path) {
      toast.error('Please select a kernel image')
      return
    }
    setLaunching(true)
    try {
      const info = await api.launchInstance({
        vcpu_count: config.vcpu_count,
        memory_mib: config.memory_mib,
        kernel_path: config.kernel_path,
        cmdline: config.cmdline,
        system_image: config.system_image || undefined,
        vendor_image: config.vendor_image || undefined,
      })
      toast.success(`Launched ${info.instance_id}`)
      navigate(`/vm/${info.instance_id}`)
    } catch (e) {
      toast.error(`Launch failed: ${e}`)
    } finally {
      setLaunching(false)
    }
  }

  return (
    <div className="p-6 max-w-3xl mx-auto">
      <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }}>
        <button onClick={() => navigate('/')} className="flex items-center gap-2 text-muted-foreground hover:text-foreground mb-6">
          <ArrowLeft className="w-4 h-4" /> Back
        </button>
        <h1 className="text-3xl font-bold mb-2">Create Virtual Machine</h1>
        <p className="text-muted-foreground mb-8">Configure a new Android emulator instance</p>

        <div className="space-y-6">
          {/* Kernel */}
          <div className="bg-card border rounded-xl p-4">
            <label className="block text-sm font-medium mb-2">Kernel (bzImage)</label>
            <div className="flex gap-2">
              <input
                type="text"
                value={config.kernel_path}
                onChange={e => setConfig(c => ({ ...c, kernel_path: e.target.value }))}
                placeholder="/path/to/bzImage"
                className="flex-1 px-3 py-2 bg-background border rounded-lg font-mono text-sm"
              />
              <button className="px-3 py-2 bg-secondary rounded-lg hover:bg-secondary/80">
                <FolderOpen className="w-4 h-4" />
              </button>
            </div>
          </div>

          {/* System image */}
          <div className="bg-card border rounded-xl p-4">
            <label className="block text-sm font-medium mb-2">System Image</label>
            <div className="flex gap-2">
              <input
                type="text"
                value={config.system_image}
                onChange={e => setConfig(c => ({ ...c, system_image: e.target.value }))}
                placeholder="/path/to/system.img"
                className="flex-1 px-3 py-2 bg-background border rounded-lg font-mono text-sm"
              />
              <button className="px-3 py-2 bg-secondary rounded-lg hover:bg-secondary/80">
                <FolderOpen className="w-4 h-4" />
              </button>
            </div>
          </div>

          {/* Resources */}
          <div className="bg-card border rounded-xl p-4">
            <h3 className="font-medium mb-4">Resources</h3>
            <div className="grid grid-cols-2 gap-4">
              <div>
                <label className="flex items-center gap-2 text-sm mb-2">
                  <Cpu className="w-4 h-4" /> vCPUs
                </label>
                <input
                  type="range"
                  min="1"
                  max="16"
                  value={config.vcpu_count}
                  onChange={e => setConfig(c => ({ ...c, vcpu_count: parseInt(e.target.value) }))}
                  className="w-full"
                />
                <p className="text-xs text-muted-foreground mt-1">{config.vcpu_count} cores</p>
              </div>
              <div>
                <label className="flex items-center gap-2 text-sm mb-2">
                  <MemoryStick className="w-4 h-4" /> Memory
                </label>
                <input
                  type="range"
                  min="512"
                  max="16384"
                  step="512"
                  value={config.memory_mib}
                  onChange={e => setConfig(c => ({ ...c, memory_mib: parseInt(e.target.value) }))}
                  className="w-full"
                />
                <p className="text-xs text-muted-foreground mt-1">{config.memory_mib} MiB</p>
              </div>
            </div>
          </div>

          {/* Launch */}
          <button
            onClick={handleLaunch}
            disabled={launching}
            className="w-full py-3 bg-primary text-primary-foreground rounded-xl font-medium hover:bg-primary/90 disabled:opacity-50"
          >
            {launching ? 'Launching...' : 'Launch Instance'}
          </button>
        </div>
      </motion.div>
    </div>
  )
}
