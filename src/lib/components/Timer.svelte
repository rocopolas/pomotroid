<script lang="ts">
  // Orchestrator component. Subscribes to timer events, owns keyboard listener,
  // and renders TimerDial + TimerDisplay + TimerFooter.
  import { onMount } from 'svelte';
  import {
    timerToggle,
    timerRestartRound,
    timerSkip,
    timerAdjustDuration,
    getTimerState,
    onTimerTick,
    onTimerPaused,
    onTimerResumed,
    onRoundChange,
    onTimerReset,
    onTimerAdjusted,
  } from '$lib/ipc';
  import { timerState } from '$lib/stores/timer';
  import { settings } from '$lib/stores/settings';
  import { fade } from 'svelte/transition';
  import TimerDial from './TimerDial.svelte';
  import TimerDisplay from './TimerDisplay.svelte';
  import TimerFooter from './TimerFooter.svelte';
  import MiniControls from './MiniControls.svelte';
  import TagPicker from './TagPicker.svelte';
  import Tooltip from './Tooltip.svelte';
  import type { UnlistenFn } from '@tauri-apps/api/event';
  import * as m from '$paraglide/messages.js';
  import { notificationShow } from '$lib/ipc';

  interface Props {
    isCompact?: boolean;
    uiScale?: number;
  }

  let { isCompact = false, uiScale = 1 }: Props = $props();

  let state = $derived($timerState);
  let remainingSecs = $derived(Math.max(0, state.total_secs - state.elapsed_secs));
  let canSubtractFive = $derived(remainingSecs > 60);
  let canAddFive = $derived(state.total_secs < 7200);

  function roundColor(rt: string): string {
    if (rt === 'work') return 'var(--color-focus-round)';
    if (rt === 'short-break') return 'var(--color-short-round)';
    return 'var(--color-long-round)';
  }

  function roundLabel(rt: string): string {
    if (rt === 'work') return m.round_label_work();
    if (rt === 'short-break') return m.round_label_short_break();
    return m.round_label_long_break();
  }

  onMount(() => {
    const cleanups: UnlistenFn[] = [];

    // Async setup: hydrate state and register event listeners.
    (async () => {
      const initial = await getTimerState();
      timerState.set(initial);

      cleanups.push(
        await onTimerTick(({ elapsed_secs, total_secs }) => {
          timerState.update((s) => ({
            ...s,
            elapsed_secs,
            total_secs,
            is_running: true,
            is_paused: false,
          }));
        }),
        await onTimerPaused(({ elapsed_secs }) => {
          timerState.update((s) => ({
            ...s,
            elapsed_secs,
            is_running: false,
            is_paused: true,
          }));
        }),
        await onTimerResumed(({ elapsed_secs }) => {
          timerState.update((s) => ({
            ...s,
            elapsed_secs,
            is_running: true,
            is_paused: false,
          }));
        }),
        await onRoundChange((snap) => {
          timerState.set(snap);
          if ($settings.notifications_enabled) {
            let title: string;
            let body: string;
            if (snap.round_type === 'work') {
              const afterBreak =
                snap.previous_round_type === 'short-break' ||
                snap.previous_round_type === 'long-break';
              title = afterBreak ? m.notification_work_title() : m.notification_work_start_title();
              body = afterBreak ? m.notification_work_body() : m.notification_work_start_body();
            } else if (snap.round_type === 'short-break') {
              title = m.notification_short_break_title();
              body = m.notification_short_break_body();
            } else {
              title = m.notification_long_break_title();
              body = m.notification_long_break_body();
            }
            notificationShow(title, body).catch(() => {});
          }
        }),
        await onTimerReset((snap) => {
          timerState.set(snap);
        }),
        await onTimerAdjusted((snap) => {
          timerState.set(snap);
        })
      );
    })();

    return () => {
      for (const unlisten of cleanups) unlisten();
    };
  });
</script>

<div class="timer-outer" class:compact={isCompact}>
  <div class="timer" style="zoom: {uiScale}">
    <!-- Dial + display stacked (display centered over dial) -->
    <div class="dial-stack">
      <TimerDial snap={state} countdown={$settings.dial_countdown} />
      <TimerDisplay {state} />
    </div>

    {#if !isCompact}
      <!-- Round label flanked by quick -5m / +5m adjust pills -->
      <div class="round-row">
        <div class="adjust-slot adjust-slot--left">
          <Tooltip text={m.tooltip_sub_5()}>
            <button
              class="btn-adjust"
              onclick={() => timerAdjustDuration(-300)}
              disabled={!canSubtractFive}
              aria-label={m.tooltip_sub_5()}
            >
              -5m
            </button>
          </Tooltip>
        </div>

        <div class="round-label" style="color: {roundColor(state.round_type)}">
          {roundLabel(state.round_type)}
        </div>

        <div class="adjust-slot adjust-slot--right">
          <Tooltip text={m.tooltip_add_5()}>
            <button
              class="btn-adjust"
              onclick={() => timerAdjustDuration(300)}
              disabled={!canAddFive}
              aria-label={m.tooltip_add_5()}
            >
              +5m
            </button>
          </Tooltip>
        </div>
      </div>

      <!-- Session tag picker -->
      <TagPicker activeTag={state.active_tag} />

      <div class="controls-wrapper">
        <!-- Back: restart current round -->
        <Tooltip text={m.tooltip_restart_round()}>
          <button class="btn-side" onclick={timerRestartRound} aria-label="Restart round">
            <svg width="18" height="18" viewBox="0 0 16 16">
              <polygon points="15,1 6,8 15,15" fill="currentColor" />
              <rect x="1" y="1" width="3" height="14" rx="1" fill="currentColor" />
            </svg>
          </button>
        </Tooltip>

        <!-- Play / Pause — icon fades when state changes -->
        <button
          class="play-pause"
          onclick={timerToggle}
          aria-label={state.is_running ? 'Pause' : 'Play'}
        >
          {#key state.is_running}
            <span class="icon" in:fade={{ duration: 120 }}>
              {#if state.is_running}
                <svg width="24" height="24" viewBox="0 0 24 24">
                  <rect x="5" y="3" width="5" height="18" rx="1.5" fill="currentColor" />
                  <rect x="14" y="3" width="5" height="18" rx="1.5" fill="currentColor" />
                </svg>
              {:else}
                <svg width="18" height="18" viewBox="0 0 24 24" style="overflow: visible;">
                  <polygon points="4,0 28,12 4,24" fill="currentColor" />
                </svg>
              {/if}
            </span>
          {/key}
        </button>

        <!-- Skip: advance to next round -->
        <Tooltip text={m.tooltip_skip()}>
          <button class="btn-side" onclick={timerSkip} aria-label="Skip round">
            <svg width="18" height="18" viewBox="0 0 16 16">
              <polygon points="1,1 10,8 1,15" fill="currentColor" />
              <rect x="12" y="1" width="3" height="14" rx="1" fill="currentColor" />
            </svg>
          </button>
        </Tooltip>

        <TimerFooter snap={state} />
      </div>
    {/if}
  </div>

  {#if isCompact}
    <MiniControls />
  {/if}
</div>

<style>
  .timer-outer {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }

  .timer {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
  }

  .dial-stack {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .controls-wrapper {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 4px 12px;
  }
  .controls-wrapper > :global(*) {
    aspect-ratio: 1;
  }

  .btn-side {
    background: none;
    border: none;
    cursor: pointer;
    color: var(--color-foreground-darker, var(--color-foreground));
    display: flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: 4px;
    transition:
      color var(--transition-default),
      background var(--transition-default);
  }

  .btn-side:hover {
    color: var(--color-foreground);
    background: var(--color-hover);
  }

  .play-pause {
    background: none;
    border: none;
    cursor: pointer;
    color: var(--color-foreground);
    display: flex;
    align-items: center;
    justify-content: center;
    width: 48px;
    height: 48px;
    /* Center within its grid cell. Without this, WebKit (macOS) places the
       fixed-size button at the start of the cell, so it renders up and to the
       left of the skip/restart buttons (#470). */
    place-self: center;
    border-radius: 50%;
    border: 2px solid var(--color-foreground-darker, var(--color-foreground));
    transition:
      color var(--transition-default),
      border-color var(--transition-default),
      background var(--transition-default);
    overflow: hidden; /* clip the fading icon within the circle */
  }

  .play-pause:hover {
    color: var(--color-accent);
    border-color: var(--color-accent);
    background: var(--color-hover);
  }

  .icon {
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .round-row {
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: center;
    width: 248px;
    margin-top: -6px;
  }

  .adjust-slot {
    display: flex;
    align-items: center;
  }

  .adjust-slot--left {
    justify-content: flex-end;
    padding-right: 12px;
  }

  .adjust-slot--right {
    justify-content: flex-start;
    padding-left: 12px;
  }

  .btn-adjust {
    background: color-mix(in oklch, var(--color-foreground) 5%, transparent);
    border: 1px solid color-mix(in oklch, var(--color-foreground) 10%, transparent);
    border-radius: 999px;
    padding: 2px 8px;
    font-size: 0.68rem;
    font-weight: 500;
    font-variant-numeric: tabular-nums;
    color: var(--color-foreground-darker, var(--color-foreground));
    cursor: pointer;
    transition:
      color var(--transition-default),
      background var(--transition-default),
      border-color var(--transition-default),
      opacity var(--transition-default);
  }

  .btn-adjust:hover:not(:disabled) {
    color: var(--color-foreground);
    background: var(--color-hover);
    border-color: color-mix(in oklch, var(--color-foreground) 22%, transparent);
  }

  .btn-adjust:disabled {
    opacity: 0.28;
    cursor: default;
  }

  .round-label {
    font-size: 0.75rem;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    text-align: center;
  }
</style>
