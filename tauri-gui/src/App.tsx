import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom'
import { Toaster } from 'sonner'
import { AnimatePresence } from 'framer-motion'
import Shell from './components/Shell'
import Dashboard from './pages/Dashboard'
import CreateVm from './pages/CreateVm'
import VmDetail from './pages/VmDetail'
import Settings from './pages/Settings'
import { ThemeProvider } from './hooks/useTheme'

export default function App() {
  return (
    <ThemeProvider>
      <BrowserRouter>
        <Shell>
          <AnimatePresence mode="wait">
            <Routes>
              <Route path="/" element={<Dashboard />} />
              <Route path="/create" element={<CreateVm />} />
              <Route path="/vm/:id" element={<VmDetail />} />
              <Route path="/settings" element={<Settings />} />
              <Route path="*" element={<Navigate to="/" replace />} />
            </Routes>
          </AnimatePresence>
        </Shell>
        <Toaster position="top-right" richColors />
      </BrowserRouter>
    </ThemeProvider>
  )
}
