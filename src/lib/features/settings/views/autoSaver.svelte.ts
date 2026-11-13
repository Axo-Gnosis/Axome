import { onDestroy } from 'svelte'

let busy = $state(0)
let flash = $state(false)
let flashTimer: ReturnType<typeof setTimeout> | undefined

function setBusy(next: boolean) {
  clearTimeout(flashTimer)
  if (next) {
    busy++
    flash = false
  } else {
    busy = Math.max(0, busy - 1)
    if (busy === 0) {
      flash = true
      flashTimer = setTimeout(() => (flash = false), 1800)
    }
  }
}

export function getSaveStatus(): 'saving' | 'saved' | 'idle' {
  return busy > 0 ? 'saving' : flash ? 'saved' : 'idle'
}

export async function trackSave<T>(promise: Promise<T>): Promise<T> {
  setBusy(true)
  try {
    return await promise
  } finally {
    setBusy(false)
  }
}

export function createAutoSaver() {
  const timers = new Map<
    string,
    {
      timeout: ReturnType<typeof setTimeout>
      task: () => Promise<unknown>
    }
  >()
  let inFlight = 0
  let localBusy = false

  function settle() {
    const nextBusy = timers.size > 0 || inFlight > 0
    if (nextBusy !== localBusy) {
      localBusy = nextBusy
      setBusy(localBusy)
    }
  }

  function runTask(key: string, task: () => Promise<unknown>) {
    timers.delete(key)
    inFlight += 1
    settle()
    void task()
      .catch(() => {
        // Callers surface their own errors (notify.error). This
        // guard just ensures an unexpected throw still settles the
        // busy flag instead of stranding the save indicator.
      })
      .finally(() => {
        inFlight = Math.max(0, inFlight - 1)
        settle()
      })
  }

  function dispose() {
    for (const [key, pending] of [...timers.entries()]) {
      clearTimeout(pending.timeout)
      runTask(key, pending.task)
    }
  }
  onDestroy(dispose)

  return {
    schedule(key: string, task: () => Promise<unknown>) {
      const existing = timers.get(key)
      if (existing) clearTimeout(existing.timeout)
      const timeout = setTimeout(() => runTask(key, task), 400)
      timers.set(key, { timeout, task })
      settle()
    }
  }
}
