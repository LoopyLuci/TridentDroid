import { useState, useEffect, useCallback } from 'react'
import { api } from '../lib/api'
import type { InstanceInfo } from '../lib/types'

export function useInstances() {
  const [instances, setInstances] = useState<InstanceInfo[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const refresh = useCallback(async () => {
    try {
      setLoading(true)
      const data = await api.listInstances()
      setInstances(data)
      setError(null)
    } catch (e) {
      setError(String(e))
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    refresh()
    const interval = setInterval(refresh, 5000)
    return () => clearInterval(interval)
  }, [refresh])

  return { instances, loading, error, refresh }
}
