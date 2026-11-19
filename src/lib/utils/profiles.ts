import type { TimerProfile } from '$lib/types';
import * as m from '$paraglide/messages.js';

/** Display name for a profile, localizing built-in presets if still using their default name. */
export function formatProfileName(profile: TimerProfile): string {
  if (profile.is_builtin) {
    if (profile.name === 'Classic') return m.profile_builtin_classic();
    if (profile.name === 'Deep Focus') return m.profile_builtin_deep_focus();
    if (profile.name === 'Sprint') return m.profile_builtin_sprint();
  }
  return profile.name;
}

/** Compact duration summary, e.g. "25/5/15m" or "50/10m". */
export function formatProfileSummary(profile: TimerProfile): string {
  const work = Math.round(profile.time_work_secs / 60);
  const parts: string[] = [String(work)];
  if (profile.short_breaks_enabled) {
    parts.push(String(Math.round(profile.time_short_break_secs / 60)));
  }
  if (profile.long_breaks_enabled) {
    parts.push(String(Math.round(profile.time_long_break_secs / 60)));
  }
  return `${parts.join('/')}m`;
}
