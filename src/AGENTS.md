## Svelte Rules

- Use Svelte 5 patterns only. Do not introduce Svelte 4 syntax in new code.
- Use `$props()` instead of `export let`.
- Use `$state()` for mutable local state.
- Use `$derived()` or `$derived.by()` for computed state. Do not store derived values in `$state()`.
- Use `$effect()` only for real side effects such as DOM work, subscriptions, or browser/Tauri API integration. Do not use it to mirror or derive state.
- Prefer `{#snippet}` and `{@render}` over slots for component composition.
- Prefer `{@attach ...}` over legacy `use:` actions when attachments are needed.
- Keep state close to where it is used. Extract shared state into `.svelte.ts` rune modules before reaching for context.
- Avoid classic Svelte stores for new shared state unless an external library forces that shape.

## SvelteKit Rules

- Do not reintroduce server-rendered assumptions.
- Do not add `+page.server.ts`, `+layout.server.ts`, `hooks.server.ts`, or remote-function-based app logic for the desktop runtime.
- Frontend data flow should go through typed Tauri invoke/channel wrappers in `src/lib`, then into component state or shared rune modules.
- Guard DOM-only or browser-only code with `onMount` or a narrowly scoped `$effect()`.
- Prefer simple route components and app-shell state over web-centric patterns such as auth redirects, SEO metadata work, or server-first data loading.
- Check existing Axome patterns before introducing a new component, state, or routing style.

## Component Reuse

- Import UI primitives from the project wrappers in `src/lib/components/ui/*`, never from `bits-ui` directly. The wrappers apply project styling tokens and establish consistent surface behavior.
- Before writing a styled `<button>`, `<input>`, `<textarea>`, or modal scaffolding (`<div class="fixed inset-0 ...">`), check `src/lib/components/ui/` for an existing primitive. Common ones: `Button`, `IconButton`, `Input`, `Textarea`, `Dialog`, `DropdownMenu`, `ContextMenu`, `Tooltip`.
- Compound wrappers export short part names and are consumed as a namespace: `import * as Dialog from '$lib/components/ui/dialog'` → `<Dialog.Root>`, `<Dialog.Content>`. Standalone components (`Button`, `InfoTooltip`, `PaletteDialog`) stay named imports.
- If a Bits UI primitive is needed and no wrapper exists for it yet, add the wrapper under `src/lib/components/ui/*` first (mirroring the sibling wrappers' pattern), then consume it from feature code.
- If the same markup/class string appears twice across components, extract a primitive before it appears a third time.

## Styling Rules

- Use Tailwind utility classes for all styling. Do not add scoped `<style>` blocks unless Tailwind cannot express the rule (e.g. pseudo-elements, complex child selectors).
- When CSS is unavoidable, use `@apply` with design tokens from `src/app.css` instead of raw hex values.
- Design tokens are defined in `src/app.css` under `@theme`. Use semantic names (`bg-surface`, `text-muted`, `border-edge`) not raw colors.
- The color palette is a warm base16 scheme. All palette colors are available as Tailwind utilities (e.g. `text-accent`, `bg-danger-bg`, `text-success`).
- Surface hierarchy: `ground` (darkest) → `surface` → `raised` → `overlay` (lightest).
- Use `group` + `group-hover:` for parent-hover-reveals (e.g. remove buttons).
- Use `transition-colors` for hover/focus color transitions — except swept lists (tree/list rows, menu items, nav lists), which take no hover transition at all (VS Code-style instant highlight) so fast mouse sweeps track the cursor.
