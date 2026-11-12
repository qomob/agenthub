import { onActivated, onBeforeUnmount, onMounted } from 'vue'

/**
 * Re-validate cached data whenever the view becomes visible again.
 *
 * The spending overview cache expires after a TTL, but a TTL alone never
 * triggers a read: it only decides what the *next* read does. A page that
 * stays mounted therefore keeps showing whatever it fetched when it was
 * entered, which is exactly the gap this composable closes.
 *
 * Two real activation events are wired:
 *  - mount / `onActivated`: the view entered the screen again. Router views
 *    without `<KeepAlive>` remount, so `onMounted` covers today's behaviour;
 *    `onActivated` covers it if the tree is ever kept alive.
 *  - window `focus`: the app window regained focus, the natural moment for a
 *    local desktop tool to re-check data other processes may have changed.
 *
 * Deliberately no polling, no timers and no database watching: a refresh is
 * never invented, it only happens on a real activation event. The store keeps
 * its TTL, so a re-activation inside the TTL window stays a cheap cache hit.
 */
export function useRefreshOnActivate(refresh: () => void, options: { focus?: boolean } = {}): void {
  const useFocus = options.focus ?? true

  function onFocus(): void {
    refresh()
  }

  onMounted(() => {
    // A view that is entered fetches immediately; the store TTL decides
    // whether that read is served from cache or refetched.
    refresh()
    if (useFocus && typeof window !== 'undefined') {
      window.addEventListener('focus', onFocus)
    }
  })

  onActivated(() => {
    // No-op unless the surrounding tree is kept alive, in which case
    // onMounted does not run again on re-entry.
    refresh()
  })

  onBeforeUnmount(() => {
    if (useFocus && typeof window !== 'undefined') {
      window.removeEventListener('focus', onFocus)
    }
  })
}
