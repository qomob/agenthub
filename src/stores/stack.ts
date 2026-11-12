import { defineStore } from 'pinia'
import { ref } from 'vue'

import { api, normalizeError } from '../lib/api'
import type { SpendingOverview, StackItemView } from '../lib/types'

/** TTL cache for the stack list, mirroring stores/skills.ts. */
const LIST_TTL_MS = 15_000

/** TTL for the spending overview. External writers (the CLI) mutate the same
 * SQLite database without notifying the GUI, so the cache must expire. */
const SPENDING_TTL_MS = 15_000

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
  /** Cache timestamp for the spending overview (0 = invalid). */
  const spendingLoadedAt = ref<number>(0)

  /** Monotonic sequence for spending requests. A response may only be applied
   * if it belongs to the most recent request — an older response that resolves
   * after invalidateSpending() or a newer fetch must never repopulate the
   * cache with stale figures. */
  let spendingSeq = 0
  /** Shared in-flight request so concurrent callers reuse one core call. */
  let spendingInFlight: Promise<SpendingOverview | null> | null = null

  /**
   * Single invalidation entry point for every successful GUI write (add/edit/
   * delete subscription, merge/unmerge, add/edit/delete item). Pages must call
   * this instead of rolling their own cache handling; the next loadSpending()
   * then refetches even inside the TTL window.
   */
  function invalidateSpending(): void {
    spendingSeq += 1 // orphan any in-flight request
    spending.value = null
    spendingLoadedAt.value = 0
    spendingError.value = null
    spendingInFlight = null
  }

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
    const cacheFresh =
      spending.value !== null && spendingLoadedAt.value > 0 && Date.now() - spendingLoadedAt.value < SPENDING_TTL_MS
    if (cacheFresh && !force) {
      return spending.value
    }
    // Reuse an identical in-flight request instead of issuing duplicates.
    if (spendingInFlight) {
      return spendingInFlight
    }
    spendingSeq += 1
    const requestSeq = spendingSeq
    spendingError.value = null
    spendingInFlight = (async () => {
      try {
        const overview = await api.spendingOverview()
        // Apply only if this is still the newest request: an older response
        // arriving after invalidateSpending() or a newer fetch is discarded.
        if (requestSeq === spendingSeq) {
          spending.value = overview
          spendingLoadedAt.value = Date.now()
        }
        return overview
      } catch (err) {
        // Failure keeps the last good figures (never a fabricated zero) and
        // only updates the error banner when this is the newest request.
        if (requestSeq === spendingSeq) {
          spendingError.value = normalizeError(err, '成本概览计算失败')
          spendingLoadedAt.value = Date.now()
        }
        return null
      } finally {
        if (requestSeq === spendingSeq) {
          spendingInFlight = null
        }
      }
    })()
    return spendingInFlight
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
    spendingLoadedAt,
    load,
    refresh,
    invalidateSpending,
    loadSpending,
    findById,
  }
})
