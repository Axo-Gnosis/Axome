import type { TabId } from '$lib/features/workbench/model'
import { ChatController } from './controller.svelte'

// Surfaces call getOrCreate inside a derived; only the controllers are reactive.
const controllers = new Map<TabId, ChatController>()

export function getOrCreate(tabId: TabId): ChatController {
  let controller = controllers.get(tabId)
  if (!controller) {
    controller = new ChatController({ kind: 'tab', tabId })
    controllers.set(tabId, controller)
  }
  return controller
}

export function get(tabId: TabId): ChatController | undefined {
  return controllers.get(tabId)
}

export function dispose(tabId: TabId): void {
  release(tabId)
}

export function release(tabId: TabId): void {
  controllers.get(tabId)?.release()
  controllers.delete(tabId)
}

export function releaseAll(): void {
  for (const controller of controllers.values()) controller.release()
  controllers.clear()
}
