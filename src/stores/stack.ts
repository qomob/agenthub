import { defineStore } from 'pinia'
import { ref } from 'vue'

import { api, normalizeError } from '../lib/api'
import type { SpendingOverview, StackItemView } from '../lib/types'

/** TTL cache for the stack list, mirroring stores/skills.ts. */
const LIST_TTL_MS = 15_000

export const useStackStore = defineStore('stack', () => {
  const items = ref<StackItemView[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)
  const loadedAt = ref<number>(0)

  const spending = ref<SpendingOverview | null>(null)
  /** Set when spending_overview fails (e.g. aggregate overflow) while the
   * item list itself is fine — the UI must keep items visible and explain the
   * failure instead of showing zero cost. */
  const spendingError = ref<string | null>(null)

  async function load(force = false): Promise<StackItemView[]> {
    if (items.value.length > 0 && !force && Date.now() - loadedAt.value < LIST_TTL_MS) {
      return items.value
    }
    loading.value = true
    error.value = null
    try {
      items.value = await api.stackList()
      loadedAt.value = Date.now()
      return items.value
    } catch (err) {
      error.value = normalizeError(err, '无法加载 AI 工具清单')
      throw err
    } finally {
      loading.value = false
    }
  }

  async function refresh(): Promise<StackItemView[]> {
    return load(true)
  }

  async function loadSpending(force = false): Promise<SpendingOverview | null> {
    if (spending.value && !force && spendingError.value === null) {
      return spending.value
    }
    spendingError.value = null
    try {
      spending.value = await api.spendingOverview()
      return spending.value
    } catch (err) {
      spendingError.value = normalizeError(err, '成本概览计算失败')
      return null
    }
  }

  function findById(id: string): StackItemView | null {
    return items.value.find((item) => item.id === id) ?? null
  }

  return {
    items,
    loading,
    error,
    loadedAt,
    spending,
    spendingError,
    load,
    refresh,
    loadSpending,
    findById,
  }
})
