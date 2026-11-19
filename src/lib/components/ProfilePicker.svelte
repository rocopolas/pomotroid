<script lang="ts">
  import { onMount } from 'svelte';
  import { profilesList, profilesApply, onProfilesChanged } from '$lib/ipc';
  import { settings } from '$lib/stores/settings';
  import { formatProfileName, formatProfileSummary } from '$lib/utils/profiles';
  import type { TimerProfile } from '$lib/types';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import * as m from '$paraglide/messages.js';

  interface Props {
    onManage: () => void;
  }

  let { onManage }: Props = $props();

  let profiles = $state<TimerProfile[]>([]);
  let open = $state(false);
  let rootEl: HTMLElement | undefined;

  let activeProfile = $derived(
    profiles.find((p) => p.id === $settings.active_profile_id) ?? null
  );
  let triggerLabel = $derived(
    activeProfile ? formatProfileName(activeProfile) : m.profile_custom()
  );

  onMount(() => {
    const cleanups: UnlistenFn[] = [];
    (async () => {
      try {
        profiles = await profilesList();
      } catch {
        // Ignore on early startup if DB is not ready.
      }
      cleanups.push(
        await onProfilesChanged((updated) => {
          profiles = updated;
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
    function handleKey(e: KeyboardEvent) {
      if (e.key === 'Escape') open = false;
    }
    window.addEventListener('mousedown', handleOutside);
    window.addEventListener('keydown', handleKey);
    return () => {
      window.removeEventListener('mousedown', handleOutside);
      window.removeEventListener('keydown', handleKey);
    };
  });

  async function selectProfile(id: number) {
    open = false;
    const updated = await profilesApply(id);
    settings.set(updated);
  }

  function handleManage() {
    open = false;
    onManage();
  }
</script>

<div class="profile-picker" bind:this={rootEl}>
  <button
    class="pill"
    class:open
    onclick={() => (open = !open)}
    aria-haspopup="listbox"
    aria-expanded={open}
    title={m.tooltip_profile_picker()}
  >
    <span class="pill-name">{triggerLabel}</span>
    <svg class="chevron" class:open width="8" height="5" viewBox="0 0 8 5" aria-hidden="true">
      <polyline
        points="1,1 4,4 7,1"
        fill="none"
        stroke="currentColor"
        stroke-width="1.3"
        stroke-linecap="round"
        stroke-linejoin="round"
      />
    </svg>
  </button>

  {#if open}
    <div class="menu" role="listbox">
      {#each profiles as p (p.id)}
        <button
          class="menu-item"
          class:active={p.id === $settings.active_profile_id}
          role="option"
          aria-selected={p.id === $settings.active_profile_id}
          onclick={() => selectProfile(p.id)}
        >
          <span class="item-name">{formatProfileName(p)}</span>
          <span class="item-meta">{formatProfileSummary(p)}</span>
        </button>
      {/each}
      <div class="divider"></div>
      <button class="menu-item manage" onclick={handleManage}>
        <span>{m.profile_manage()}</span>
      </button>
    </div>
  {/if}
</div>

<style>
  .profile-picker {
    position: relative;
    display: flex;
    align-items: center;
    -webkit-app-region: no-drag;
  }

  .pill {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 9px;
    border-radius: 999px;
    border: 1px solid transparent;
    background: color-mix(in oklch, var(--color-foreground) 5%, transparent);
    color: var(--color-foreground-darker, var(--color-foreground));
    font-size: 0.7rem;
    font-weight: 500;
    letter-spacing: 0.02em;
    cursor: pointer;
    max-width: 145px;
    transition:
      color var(--transition-default),
      background var(--transition-default),
      border-color var(--transition-default);
  }

  .pill:hover,
  .pill.open {
    color: var(--color-foreground);
    background: var(--color-hover);
    border-color: color-mix(in oklch, var(--color-foreground) 12%, transparent);
  }

  .pill-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chevron {
    flex-shrink: 0;
    opacity: 0.7;
    transition: transform 0.15s ease;
  }

  .chevron.open {
    transform: rotate(180deg);
  }

  .menu {
    position: absolute;
    top: calc(100% + 6px);
    left: 50%;
    transform: translateX(-50%);
    min-width: 186px;
    max-width: 230px;
    padding: 4px;
    border-radius: 8px;
    background: var(--color-background-light, var(--color-background));
    border: 1px solid color-mix(in oklch, var(--color-foreground) 14%, transparent);
    box-shadow: 0 8px 24px color-mix(in oklch, black 35%, transparent);
    z-index: 200;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .menu-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    width: 100%;
    padding: 6px 9px;
    border: none;
    border-radius: 5px;
    background: none;
    color: var(--color-foreground);
    font-size: 0.75rem;
    text-align: left;
    cursor: pointer;
    transition: background 0.12s;
  }

  .menu-item:hover {
    background: var(--color-hover);
  }

  .menu-item.active {
    color: var(--color-accent);
    background: color-mix(in oklch, var(--color-accent) 12%, transparent);
  }

  .item-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
  }

  .item-meta {
    font-size: 0.68rem;
    font-variant-numeric: tabular-nums;
    color: var(--color-foreground-darker, var(--color-foreground));
    opacity: 0.75;
    flex-shrink: 0;
  }

  .divider {
    height: 1px;
    background: var(--color-separator);
    margin: 3px 4px;
  }

  .menu-item.manage {
    color: var(--color-foreground-darker, var(--color-foreground));
    font-size: 0.72rem;
  }

  .menu-item.manage:hover {
    color: var(--color-foreground);
  }
</style>
