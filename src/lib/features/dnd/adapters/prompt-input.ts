import { DND_MIME, type DragPayload } from '$lib/features/dnd/payload'
import { dragObserver } from '$lib/features/dnd/observer.svelte'
import { DropRegistry } from '$lib/features/dnd/registry.svelte'

interface PromptInputDropArgs {
  id: string
  canAcceptDrop: () => boolean
  onDrop: (payload: DragPayload, clientX: number, clientY: number) => void | Promise<void>
}

const accepts = (payload: DragPayload) => payload.items.some((item) => item.kind === 'file' || item.kind === 'os-files')

/** Explorer files (HTML5) and OS files (Tauri) dropped on the composer; pasted or browser-dropped `Files` stay with the editor. */
export function promptInputDropTarget(args: PromptInputDropArgs) {
  const observer = dragObserver({
    accept: (payload, types) =>
      args.canAcceptDrop() && (payload ? accepts(payload) : types.includes(DND_MIME.AXOME_FILE)),
    onDrop: (event, payload) =>
      args.canAcceptDrop() && accepts(payload) ? args.onDrop(payload, event.clientX, event.clientY) : undefined
  })
  return (element: HTMLElement) => {
    const disposeObserver = observer(element)
    const disposeRegistry = DropRegistry.register({
      id: `prompt-input:${args.id}`,
      element,
      accept: (payload) => args.canAcceptDrop() && accepts(payload),
      dispatch: (payload, x, y) => args.onDrop(payload, x, y)
    })
    return () => {
      disposeRegistry()
      disposeObserver()
    }
  }
}
