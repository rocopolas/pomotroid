<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { tagsList, timerSetTag, tagsDelete, onTagsChanged } from '$lib/ipc';
  import type { Tag } from '$lib/types';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import * as m from '$paraglide/messages.js';

  interface Props {
    activeTag: string | null;
  }

  let { activeTag }: Props = $props();

  let tags = $state<Tag[]>([]);
  let open = $state(false);
  let draft = $state('');
  let rootEl = $state<HTMLElement | undefined>(undefined);
  let inputEl = $state<HTMLInputElement | undefined>(undefined);

  let filteredTags = $derived.by(() => {
    const q = draft.trim().toLowerCase();
    if (!q) return tags;
    return tags.filter((t) => t.name.toLowerCase().includes(q));
  });

  onMount(() => {
    const cleanups: UnlistenFn[] = [];
    (async () => {
      try {
        tags = await tagsList();
      } catch {
        // Ignore on early startup.
      }
      cleanups.push(
        await onTagsChanged((updated) => {
          tags = updated;
        })
      );
    })();
    return () => {
      for (const fn of cleanups) fn();
    };
  });

  $effect(() => {
    if (!open) return;
    function handleOutside(e: MouseEvent) {
      if (rootEl && !rootEl.contains(e.target as Node)) open = false;
    }
    window.addEventListener('mousedown', handleOutside);
    return () => window.removeEventListener('mousedown', handleOutside);
  });

  async function toggleOpen() {
    open = !open;
    if (open) {
      draft = '';
      await tick();
      inputEl?.focus();
    }
  }

  async function pickTag(name: string | null) {
    open = false;
    draft = '';
    await timerSetTag(name);
  }

  async function submitDraft() {
    const clean = draft.trim();
    if (!clean) {
      open = false;
      return;
    }
    await pickTag(clean);
  }

  async function handleClear(e: MouseEvent) {
    e.stopPropagation();
    await timerSetTag(null);
  }

  async function handleDeleteSaved(e: MouseEvent, id: number) {
    e.stopPropagation();
    await tagsDelete(id);
  }
</script>

<div class="tag-picker" bind:this={rootEl}>
  {#if activeTag}
    <div class="active-pill">
      <button class="pill-main" onclick={toggleOpen} title={m.tag_add()}>
        <span class="hash">#</span>
        <span class="pill-text">{activeTag}</span>
      </button>
      <button class="pill-clear" onclick={handleClear} aria-label={m.tag_clear()} title={m.tag_clear()}>
        ×
      </button>
    </div>
  {:else}
    <button class="ghost-pill" onclick={toggleOpen}>
      <span class="plus">+</span>
      <span>{m.tag_add()}</span>
    </button>
  {/if}

  {#if open}
    <div class="popover">
      <input
        bind:this={inputEl}
        class="tag-input"
        type="text"
        maxlength="32"
        placeholder={m.tag_placeholder()}
        bind:value={draft}
        onkeydown={(e) => {
          if (e.key === 'Enter') {
            e.preventDefault();
            submitDraft();
          } else if (e.key === 'Escape') {
            e.preventDefault();
            open = false;
          }
        }}
      />

      {#if filteredTags.length > 0}
        <div class="tag-list">
          {#each filteredTags as t (t.id)}
            <div class="tag-row" class:selected={activeTag?.toLowerCase() === t.name.toLowerCase()}>
              <button class="tag-select" onclick={() => pickTag(t.name)}>
                <span class="hash">#</span>
                <span class="tag-name">{t.name}</span>
              </button>
              <button
                class="tag-del"
                onclick={(e) => handleDeleteSaved(e, t.id)}
                aria-label="Delete tag"
                title="Eliminar"
              >
                ×
              </button>
            </div>
          {/each}
        </div>
      {:else if !draft.trim()}
        <div class="empty-hint">{m.tag_empty_hint()}</div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .tag-picker {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    margin-top: -6px;
  }

  .ghost-pill {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 9px;
    border-radius: 999px;
    border: 1px dashed color-mix(in oklch, var(--color-foreground) 16%, transparent);
    background: none;
    color: var(--color-foreground-darker, var(--color-foreground));
    opacity: 0.6;
    font-size: 0.68rem;
    cursor: pointer;
    transition:
      opacity 0.15s,
      color 0.15s,
      border-color 0.15s,
      background 0.15s;
  }

  .ghost-pill:hover {
    opacity: 1;
    color: var(--color-foreground);
    border-color: color-mix(in oklch, var(--color-foreground) 30%, transparent);
    background: var(--color-hover);
  }

  .plus {
    font-weight: 600;
    line-height: 1;
  }

  .active-pill {
    display: inline-flex;
    align-items: center;
    border-radius: 999px;
    background: color-mix(in oklch, var(--color-accent) 16%, transparent);
    border: 1px solid color-mix(in oklch, var(--color-accent) 38%, transparent);
    color: var(--color-foreground);
    max-width: 190px;
    overflow: hidden;
  }

  .pill-main {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 2px 6px 2px 9px;
    background: none;
    border: none;
    color: inherit;
    font-size: 0.7rem;
    font-weight: 500;
    cursor: pointer;
    min-width: 0;
  }

  .pill-main:hover {
    color: var(--color-accent);
  }

  .hash {
    opacity: 0.65;
    font-weight: 600;
  }

  .pill-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .pill-clear {
    background: none;
    border: none;
    color: var(--color-foreground-darker, var(--color-foreground));
    font-size: 0.85rem;
    line-height: 1;
    padding: 2px 7px 2px 2px;
    cursor: pointer;
    transition: color 0.12s;
  }

  .pill-clear:hover {
    color: var(--color-foreground);
  }

  .popover {
    position: absolute;
    top: calc(100% + 6px);
    left: 50%;
    transform: translateX(-50%);
    width: 196px;
    padding: 6px;
    border-radius: 8px;
    background: var(--color-background-light, var(--color-background));
    border: 1px solid color-mix(in oklch, var(--color-foreground) 14%, transparent);
    box-shadow: 0 8px 24px color-mix(in oklch, black 35%, transparent);
    z-index: 150;
    display: flex;
    flex-direction: column;
    gap: 5px;
  }

  .tag-input {
    width: 100%;
    padding: 5px 8px;
    border-radius: 5px;
    border: 1px solid color-mix(in oklch, var(--color-foreground) 16%, transparent);
    background: var(--color-background);
    color: var(--color-foreground);
    font-size: 0.74rem;
    outline: none;
  }

  .tag-input:focus {
    border-color: var(--color-accent);
  }

  .tag-list {
    display: flex;
    flex-direction: column;
    gap: 1px;
    max-height: 112px;
    overflow-y: auto;
  }

  .tag-row {
    display: flex;
    align-items: center;
    border-radius: 4px;
    transition: background 0.12s;
  }

  .tag-row:hover {
    background: var(--color-hover);
  }

  .tag-row.selected {
    background: color-mix(in oklch, var(--color-accent) 14%, transparent);
  }

  .tag-select {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 6px;
    background: none;
    border: none;
    color: var(--color-foreground);
    font-size: 0.73rem;
    text-align: left;
    cursor: pointer;
    min-width: 0;
  }

  .tag-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tag-del {
    background: none;
    border: none;
    color: var(--color-foreground-darker, var(--color-foreground));
    opacity: 0;
    padding: 2px 6px;
    font-size: 0.8rem;
    cursor: pointer;
    transition: opacity 0.12s, color 0.12s;
  }

  .tag-row:hover .tag-del {
    opacity: 0.7;
  }

  .tag-del:hover {
    opacity: 1 !important;
    color: var(--color-focus-round);
  }

  .empty-hint {
    font-size: 0.68rem;
    color: var(--color-foreground-darker, var(--color-foreground));
    opacity: 0.7;
    padding: 4px 6px;
    text-align: center;
  }
</style>
