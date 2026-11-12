<script lang="ts">
  import * as api from "./api";
  import { formatStartTime } from "./format";

  let {
    ids,
    defaultTime,
    onclose,
  }: { ids: number[]; defaultTime: string; onclose: () => void } = $props();

  // svelte-ignore state_referenced_locally
  let time = $state(defaultTime);
  let error = $state<string | null>(null);
  let busy = $state(false);

  /** The next moment the clock shows `time`: today, or tomorrow if it has passed. */
  const startAt = $derived.by(() => {
    const [h, m] = time.split(":").map(Number);
    if (Number.isNaN(h) || Number.isNaN(m)) return null;
    const at = new Date();
    at.setHours(h, m, 0, 0);
    if (at.getTime() <= Date.now()) at.setDate(at.getDate() + 1);
    return Math.floor(at.getTime() / 1000);
  });

  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (startAt == null || busy) return;
    busy = true;
    try {
      await api.scheduleDownloads(ids, startAt);
      onclose();
    } catch (err) {
      error = api.message(err);
    } finally {
      busy = false;
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && onclose()}>
  <form class="modal narrow" aria-label="Schedule" onsubmit={save}>
    <h2>Schedule {ids.length === 1 ? "download" : `${ids.length} downloads`}</h2>
    <label class="field">
      <span>Start at</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input type="time" bind:value={time} required autofocus />
    </label>
    <p class="hint">
      {#if startAt != null}Joins the queue at <b>{formatStartTime(startAt)}</b>.{/if}
      Running downloads pause until then and keep their progress.
    </p>
    {#if error}
      <p class="error-text">{error}</p>
    {/if}
    <div class="actions">
      <button type="button" class="btn" onclick={onclose}>Cancel</button>
      <button type="submit" class="btn primary" disabled={startAt == null || busy}>Schedule</button>
    </div>
  </form>
</div>

<style>
  .narrow {
    width: min(380px, calc(100vw - 32px));
  }
  .hint {
    margin: 0;
    color: var(--muted);
  }
  p {
    margin: 0;
  }
</style>
