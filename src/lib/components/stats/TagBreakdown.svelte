<script lang="ts">
  import type { TagStat } from '$lib/types';
  import * as m from '$paraglide/messages.js';

  interface Props {
    items: TagStat[];
  }

  let { items }: Props = $props();

  let maxMins = $derived(Math.max(1, ...items.map((i) => i.focus_mins)));

  function fmtTime(mins: number): string {
    if (mins < 60) return `${mins}m`;
    const h = Math.floor(mins / 60);
    const rem = mins % 60;
    return rem === 0 ? `${h}h` : `${h}h ${rem}m`;
  }
</script>

{#if items.length > 0}
  <div class="tag-breakdown">
    <span class="heading">{m.stats_by_tag()}</span>
    <div class="rows">
      {#each items as item}
        {@const pct = Math.max(4, Math.round((item.focus_mins / maxMins) * 100))}
        <div class="row">
          <span class="name" class:untagged={item.tag === null}>
            {item.tag ? `#${item.tag}` : m.tag_untagged()}
          </span>
          <div class="bar-track">
            <div class="bar-fill" style="width: {pct}%"></div>
          </div>
          <span class="meta">{fmtTime(item.focus_mins)} · {item.rounds}</span>
        </div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .tag-breakdown {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 24px 16px;
    border-top: 1px solid var(--color-separator);
    flex-shrink: 0;
  }

  .heading {
    font-size: 0.67rem;
    font-weight: 600;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--color-foreground-darker);
  }

  .rows {
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-height: 92px;
    overflow-y: auto;
  }

  .row {
    display: grid;
    grid-template-columns: 120px 1fr auto;
    align-items: center;
    gap: 12px;
    font-size: 0.74rem;
  }

  .name {
    color: var(--color-foreground);
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .name.untagged {
    color: var(--color-foreground-darker);
    font-style: italic;
    font-weight: 400;
  }

  .bar-track {
    height: 6px;
    border-radius: 3px;
    background: color-mix(in oklch, var(--color-foreground) 8%, transparent);
    overflow: hidden;
  }

  .bar-fill {
    height: 100%;
    border-radius: 3px;
    background: var(--color-focus-round);
    transition: width 0.25s ease;
  }

  .meta {
    font-size: 0.72rem;
    font-variant-numeric: tabular-nums;
    color: var(--color-foreground-darker);
    min-width: 72px;
    text-align: right;
  }
</style>
