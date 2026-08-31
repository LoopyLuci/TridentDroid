import { useParams, useNavigate } from 'react-router-dom';
import { motion } from 'framer-motion';
import { ArrowLeft, Terminal, Monitor, Activity } from 'lucide-react';
import { useState } from 'react';
import TerminalComponent from '../components/Terminal';
import Display from '../components/Display';

export default function VmDetail() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const [tab, setTab] = useState<'terminal' | 'display' | 'metrics'>('terminal');

  return (
    <div className="p-6 h-screen flex flex-col">
      <motion.div initial={{ opacity: 0 }} animate={{ opacity: 1 }} className="flex-1 flex flex-col min-h-0">
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
        <div className="flex-1 min-h-0 bg-card border rounded-xl overflow-hidden">
          {tab === 'terminal' && (
            <div className="h-full p-2">
              <TerminalComponent instanceId={id || ''} />
            </div>
          )}
          {tab === 'display' && (
            <div className="h-full p-4 flex items-center justify-center">
              <Display instanceId={id || ''} />
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
  );
}
