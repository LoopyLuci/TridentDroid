import { useParams, useNavigate } from 'react-router-dom'
import { motion } from 'framer-motion'
import { ArrowLeft, Terminal, Monitor, Activity, Send } from 'lucide-react'
import { useState } from 'react'
import { api } from '../lib/api'

export default function VmDetail() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()
  const [tab, setTab] = useState<'terminal' | 'display' | 'metrics'>('terminal')
  const [command, setCommand] = useState('')
  const [output, setOutput] = useState('')

  const handleCommand = async () => {
    if (!command || !id) return
    const result = await api.adbShell(id, command)
    setOutput(prev => prev + `$ ${command}
${result}
`)
    setCommand('')
  }

  return (
    <div className="p-6 h-screen flex flex-col">
      <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }}>
        <button onClick={() => navigate('/')} className="flex items-center gap-2 text-muted-foreground hover:text-foreground mb-4">
          <ArrowLeft className="w-4 h-4" /> Back
        </button>
        <div className="flex items-center justify-between mb-4">
          <h1 className="text-2xl font-bold">{id}</h1>
        </div>

        {/* Tabs */}
        <div className="flex gap-2 mb-4">
          <button
            onClick={() => setTab('terminal')}
            className={`flex items-center gap-2 px-4 py-2 rounded-lg ${tab === 'terminal' ? 'bg-primary text-primary-foreground' : 'bg-secondary'}`}
          >
            <Terminal className="w-4 h-4" /> Terminal
          </button>
          <button
            onClick={() => setTab('display')}
            className={`flex items-center gap-2 px-4 py-2 rounded-lg ${tab === 'display' ? 'bg-primary text-primary-foreground' : 'bg-secondary'}`}
          >
            <Monitor className="w-4 h-4" /> Display
          </button>
          <button
            onClick={() => setTab('metrics')}
            className={`flex items-center gap-2 px-4 py-2 rounded-lg ${tab === 'metrics' ? 'bg-primary text-primary-foreground' : 'bg-secondary'}`}
          >
            <Activity className="w-4 h-4" /> Metrics
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 bg-card border rounded-xl overflow-hidden">
          {tab === 'terminal' && (
            <div className="h-full flex flex-col">
              <div className="flex-1 p-4 font-mono text-sm bg-background overflow-auto">
                <pre className="whitespace-pre-wrap">{output || 'Ready. Type a command below.\n'}</pre>
              </div>
              <div className="p-2 border-t flex gap-2">
                <input
                  type="text"
                  value={command}
                  onChange={e => setCommand(e.target.value)}
                  onKeyDown={e => e.key === 'Enter' && handleCommand()}
                  placeholder="Enter command..."
                  className="flex-1 px-3 py-2 bg-background border rounded-lg font-mono text-sm"
                />
                <button onClick={handleCommand} className="px-4 py-2 bg-primary text-primary-foreground rounded-lg">
                  <Send className="w-4 h-4" />
                </button>
              </div>
            </div>
          )}
          {tab === 'display' && (
            <div className="h-full flex items-center justify-center bg-black">
              <p className="text-white/50">Display stream not connected</p>
            </div>
          )}
          {tab === 'metrics' && (
            <div className="p-6">
              <p className="text-muted-foreground">Metrics not available yet</p>
            </div>
          )}
        </div>
      </motion.div>
    </div>
  )
}
