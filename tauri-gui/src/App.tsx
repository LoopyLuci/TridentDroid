import { useState } from 'react';
import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom';
import { Toaster } from 'sonner';
import { AnimatePresence } from 'framer-motion';
import Shell from './components/Shell';
import Dashboard from './pages/Dashboard';
import CreateVm from './pages/CreateVm';
import VmDetail from './pages/VmDetail';
import Settings from './pages/Settings';
import SplashScreen from './components/SplashScreen';
import { ThemeProvider } from './hooks/useTheme';

export default function App() {
  const [showSplash, setShowSplash] = useState(true);

  return (
    <ThemeProvider>
      <AnimatePresence mode="wait">
        {showSplash && (
          <SplashScreen key="splash" onFinish={() => setShowSplash(false)} />
        )}
      </AnimatePresence>
      
      {!showSplash && (
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
      )}
    </ThemeProvider>
  );
}
