<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import * as api from "$lib/api";
  import { extension, formatBytes, formatEta, formatSpeed, percent } from "$lib/format";
  import Icon, { type IconName } from "$lib/Icon.svelte";
  import AddDialog from "$lib/AddDialog.svelte";
  import SettingsDialog from "$lib/SettingsDialog.svelte";

  type Filter = "all" | "active" | "completed" | "failed";

  const FILTERS: { id: Filter; label: string; icon: IconName }[] = [
    { id: "all", label: "All downloads", icon: "list" },
    { id: "active", label: "Unfinished", icon: "activity" },
    { id: "completed", label: "Finished", icon: "check" },
    { id: "failed", label: "Failed", icon: "alert" },
  ];

  const STATUS_LABEL: Record<api.Status, string> = {
    queued: "Waiting",
    downloading: "Downloading",
    paused: "Paused",
    completed: "Finished",
    failed: "Failed",
  };

  let downloads = $state<api.Download[]>([]);
  let settings = $state<api.Settings | null>(null);
  let queue = $state<api.QueueState>({ running: true, resumesAt: null });
  let filter = $state<Filter>("all");
  const selected = new SvelteSet<number>();
  let lastClicked: number | null = null;

  let showAdd = $state(false);
  let addUrl = $state("");
  let showSettings = $state(false);
  let confirmRemove = $state(false);
  let deleteFiles = $state(false);
  let banner = $state<string | null>(null);

  const matches = (d: api.Download, f: Filter) =>
    f === "all" ||
    (f === "active" && ["queued", "downloading", "paused"].includes(d.status)) ||
    (f === "completed" && d.status === "completed") ||
    (f === "failed" && d.status === "failed");

  const visible = $derived(downloads.filter((d) => matches(d, filter)).sort((a, b) => b.id - a.id));
  const counts = $derived(
    Object.fromEntries(FILTERS.map((f) => [f.id, downloads.filter((d) => matches(d, f.id)).length])),
  );
  const selection = $derived(downloads.filter((d) => selected.has(d.id)));
  const canStart = $derived(
    selection.some((d) => d.status === "paused" || d.status === "failed"),
  );
  const canStartNow = $derived(
    selection.some((d) => d.status !== "completed" && d.status !== "downloading"),
  );
  const waiting = $derived(downloads.filter((d) => d.status === "queued").length);
  const canPause = $derived(selection.some((d) => d.status === "downloading" || d.status === "queued"));
  const running = $derived(downloads.filter((d) => d.status === "downloading"));
  const totalSpeed = $derived(running.reduce((sum, d) => sum + (d.speed ?? 0), 0));

  async function run(action: () => Promise<unknown>) {
    try {
      banner = null;
      await action();
    } catch (e) {
      banner = api.message(e);
    }
  }

  function find(id: number) {
    return downloads.find((d) => d.id === id);
  }

  async function handleEvent(ev: api.EngineEvent) {
    switch (ev.type) {
      case "added": {
        const d = await api.getDownload(ev.id);
        if (!find(d.id)) downloads.push(d);
        break;
      }
      case "status": {
        const d = find(ev.id);
        if (!d) break;
        d.status = ev.status;
        d.error = ev.error;
        if (ev.status !== "downloading") d.speed = 0;
        if (ev.status === "completed") Object.assign(d, await api.getDownload(ev.id), { speed: 0 });
        break;
      }
      case "progress": {
        const d = find(ev.id);
        if (!d) break;
        d.downloaded = ev.downloaded;
        d.size = ev.total ?? d.size;
        d.speed = d.status === "downloading" ? ev.speed : 0;
        break;
      }
      case "removed":
        downloads = downloads.filter((d) => d.id !== ev.id);
        selected.delete(ev.id);
        break;
    }
  }

  onMount(() => {
    let unlisten: (() => void) | undefined;
    let disposed = false;
    let unlistenQueue: (() => void) | undefined;
    (async () => {
      unlisten = await api.onEngineEvent(handleEvent);
      unlistenQueue = await api.onQueueState((s) => (queue = s));
      if (disposed) {
        unlisten();
        unlistenQueue();
      }
      [downloads, settings, queue] = await Promise.all([
        api.listDownloads(),
        api.getSettings(),
        api.queueState(),
      ]);
    })().catch((e) => (banner = api.message(e)));
    return () => {
      disposed = true;
      unlisten?.();
      unlistenQueue?.();
    };
  });

  function select(e: MouseEvent, id: number) {
    if (e.shiftKey && lastClicked !== null) {
      const ids = visible.map((d) => d.id);
      const [a, b] = [ids.indexOf(lastClicked), ids.indexOf(id)].sort((x, y) => x - y);
      if (a >= 0) {
        if (!e.ctrlKey && !e.metaKey) selected.clear();
        ids.slice(a, b + 1).forEach((i) => selected.add(i));
        return;
      }
    }
    if (e.ctrlKey || e.metaKey) {
      if (selected.has(id)) selected.delete(id);
      else selected.add(id);
    } else {
      selected.clear();
      selected.add(id);
    }
    lastClicked = id;
  }

  function activate(d: api.Download) {
    if (d.status === "completed") run(() => api.openFile(d.id));
    else if (d.status !== "downloading") run(() => api.startNow([d.id]));
  }

  function openAdd(url = "") {
    addUrl = url;
    showAdd = true;
  }

  async function onPaste(e: ClipboardEvent) {
    const target = e.target as HTMLElement;
    if (showAdd || showSettings || target.closest("input")) return;
    const text = e.clipboardData?.getData("text")?.trim() ?? "";
    if (/^(https?|ftp):\/\/\S+$/i.test(text)) openAdd(text);
  }

  function onkeydown(e: KeyboardEvent) {
    if (showAdd || showSettings || confirmRemove) return;
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "n") {
      e.preventDefault();
      openAdd();
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "a") {
      e.preventDefault();
      visible.forEach((d) => selected.add(d.id));
    } else if (e.key === "Delete" && selected.size) {
      confirmRemove = true;
    } else if (e.key === "Enter" && selection.length === 1) {
      activate(selection[0]);
    }
  }

  async function removeSelected() {
    const ids = [...selected];
    confirmRemove = false;
    await run(() => api.removeDownloads(ids, deleteFiles));
    deleteFiles = false;
  }
</script>

<svelte:window {onkeydown} onpaste={onPaste} />

<div class="app">
  <header class="toolbar">
    <div class="brand">
      <span class="logo"><Icon name="download" size={16} /></span>
      Ferry
    </div>
    <button class="btn primary" onclick={() => openAdd()} title="Add a download (Ctrl+N)">
      <Icon name="plus" size={16} /> Add URL
    </button>
    <div class="sep"></div>
    <button
      class="tool"
      disabled={!canStart}
      title="Add to the queue; starts when there is a free slot"
      onclick={() => run(() => api.startDownloads([...selected]))}>
      <Icon name="play" /> Resume
    </button>
    <button
      class="tool"
      disabled={!canStartNow}
      title="Start right away, ignoring the queue limit and schedule (double-click)"
      onclick={() => run(() => api.startNow([...selected]))}>
      <Icon name="zap" /> Start now
    </button>
    <button class="tool" disabled={!canPause} onclick={() => run(() => api.pauseDownloads([...selected]))}>
      <Icon name="pause" /> Pause
    </button>
    <button class="tool" disabled={!selected.size} onclick={() => (confirmRemove = true)}>
      <Icon name="trash" /> Remove
    </button>
    <div class="sep"></div>
    <button
      class="tool"
      disabled={selection.length !== 1}
      onclick={() => run(() => api.showInFolder(selection[0].id))}>
      <Icon name="folder" /> Show in folder
    </button>
    <div class="spacer"></div>
    <button class="tool" disabled={!settings} onclick={() => (showSettings = true)}>
      <Icon name="sliders" /> Settings
    </button>
  </header>

  <div class="body">
    <nav class="sidebar" aria-label="Categories">
      {#each FILTERS as f (f.id)}
        <button class="cat" class:active={filter === f.id} onclick={() => (filter = f.id)}>
          <Icon name={f.icon} size={16} />
          <span>{f.label}</span>
          <span class="count">{counts[f.id]}</span>
        </button>
      {/each}
    </nav>

    <main class="list" aria-label="Downloads">
      {#if banner}
        <div class="banner" role="alert">
          <Icon name="alert" size={16} />
          <span>{banner}</span>
          <button class="icon-btn" aria-label="Dismiss" onclick={() => (banner = null)}>
            <Icon name="x" size={14} />
          </button>
        </div>
      {/if}

      <div class="thead">
        <span>Name</span>
        <span class="num">Size</span>
        <span>Progress</span>
        <span class="num">Speed</span>
        <span class="num">Time left</span>
        <span>Status</span>
      </div>

      {#if visible.length === 0}
        <div class="empty">
          <div class="empty-icon"><Icon name="download" size={28} /></div>
          {#if downloads.length === 0}
            <p><b>No downloads yet</b></p>
            <p>Click <b>Add URL</b> or paste a link anywhere in this window.</p>
          {:else}
            <p>Nothing in this category.</p>
          {/if}
        </div>
      {:else}
        <div class="rows" role="listbox" aria-multiselectable="true" tabindex="-1">
          {#each visible as d (d.id)}
            {@const pct = percent(d.downloaded, d.size)}
            <div
              class="tr"
              class:selected={selected.has(d.id)}
              role="option"
              tabindex="-1"
              aria-selected={selected.has(d.id)}
              onclick={(e) => select(e, d.id)}
              ondblclick={() => activate(d)}
              onkeydown={() => {}}>
              <span class="name" title={`${d.file_name}\n${d.url}`}>
                <span class="ext">{extension(d.file_name) || "FILE"}</span>
                <span class="fname">{d.file_name}</span>
              </span>
              <span class="num">{formatBytes(d.size)}</span>
              <span class="progress">
                <span class="bar" class:indeterminate={pct === null && d.status === "downloading"}>
                  <span class="fill {d.status}" style:width="{pct ?? 0}%"></span>
                </span>
                <span class="pct">
                  {pct === null ? formatBytes(d.downloaded) : `${pct.toFixed(pct >= 99.95 ? 0 : 1)}%`}
                </span>
              </span>
              <span class="num">{d.status === "downloading" ? formatSpeed(d.speed) : ""}</span>
              <span class="num">
                {d.status === "downloading" && d.size != null ? formatEta(d.size - d.downloaded, d.speed) : ""}
              </span>
              <span class="status {d.status}" title={d.error ?? ""}>
                {STATUS_LABEL[d.status]}
              </span>
            </div>
          {/each}
        </div>
      {/if}
    </main>
  </div>

  <footer class="statusbar">
    <span>{downloads.length} downloads</span>
    <span>{running.length} active</span>
    {#if waiting}<span>{waiting} waiting</span>{/if}
    {#if totalSpeed}<span class="speed">↓ {formatSpeed(totalSpeed)}</span>{/if}
    {#if settings?.speedLimitKib}
      <span title="Total speed limit (Settings)">limit {formatSpeed(settings.speedLimitKib * 1024)}</span>
    {/if}
    {#if !queue.running}
      <span class="held" title="The queue only runs inside the scheduled hours (Settings)">
        <Icon name="clock" size={13} />
        Queue on hold{queue.resumesAt ? ` until ${queue.resumesAt}` : ""}
      </span>
    {/if}
    {#if selection.length === 1 && selection[0].error}
      <span class="err" title={selection[0].error}>{selection[0].error}</span>
    {/if}
  </footer>
</div>

{#if showAdd && settings}
  <AddDialog defaultDir={settings.downloadDir} initialUrl={addUrl} onclose={() => (showAdd = false)} />
{/if}

{#if showSettings && settings}
  <SettingsDialog {settings} onsave={(s) => (settings = s)} onclose={() => (showSettings = false)} />
{/if}

{#if confirmRemove}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && (confirmRemove = false)}>
    <div class="modal" role="alertdialog" aria-label="Remove downloads">
      <h2>Remove {selected.size === 1 ? "this download" : `${selected.size} downloads`}?</h2>
      <p class="hint">Unfinished parts are always deleted.</p>
      <label class="check">
        <input type="checkbox" bind:checked={deleteFiles} />
        Also delete finished files from disk
      </label>
      <div class="actions">
        <button class="btn" onclick={() => (confirmRemove = false)}>Cancel</button>
        <button class="btn primary" onclick={removeSelected}>Remove</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .app {
    display: grid;
    grid-template-rows: auto 1fr auto;
    height: 100vh;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 8px 12px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 172px;
    font-weight: 650;
    font-size: 14px;
    letter-spacing: 0.2px;
  }
  .logo {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 7px;
    background: var(--accent);
    color: var(--accent-text);
  }
  .sep {
    width: 1px;
    height: 22px;
    background: var(--border);
    margin: 0 6px;
  }
  .spacer {
    flex: 1;
  }
  .tool {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    cursor: pointer;
    white-space: nowrap;
  }
  .tool:hover:not(:disabled) {
    background: var(--surface-2);
  }
  .tool:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .body {
    display: grid;
    grid-template-columns: 196px 1fr;
    min-height: 0;
  }

  .sidebar {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 10px 8px;
    border-right: 1px solid var(--border);
    background: var(--surface);
  }
  .cat {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 10px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    text-align: left;
  }
  .cat span:nth-child(2) {
    flex: 1;
  }
  .cat:hover {
    background: var(--surface-2);
    color: var(--text);
  }
  .cat.active {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .count {
    font-size: 11.5px;
    font-variant-numeric: tabular-nums;
  }

  .list {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }

  .thead,
  .tr {
    display: grid;
    grid-template-columns: minmax(180px, 1fr) 84px minmax(120px, 190px) 90px 80px 96px;
    gap: 12px;
    align-items: center;
    padding: 0 16px;
  }
  .thead {
    height: 32px;
    color: var(--muted);
    font-size: 11.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    border-bottom: 1px solid var(--border);
  }
  .rows {
    overflow-y: auto;
    flex: 1;
    outline: none;
  }
  .tr {
    height: 38px;
    border-bottom: 1px solid var(--border);
    cursor: default;
  }
  .tr:hover {
    background: var(--surface);
  }
  .tr.selected {
    background: var(--accent-soft);
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .name {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .ext {
    flex: none;
    width: 38px;
    padding: 2px 0;
    border-radius: 4px;
    background: var(--surface-2);
    color: var(--muted);
    font-size: 9.5px;
    font-weight: 700;
    text-align: center;
    letter-spacing: 0.3px;
  }
  .fname {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .progress {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .bar {
    position: relative;
    flex: 1;
    height: 6px;
    border-radius: 3px;
    background: var(--track);
    overflow: hidden;
  }
  .fill {
    position: absolute;
    inset: 0 auto 0 0;
    border-radius: 3px;
    background: var(--muted);
    transition: width 0.4s ease;
  }
  .fill.downloading {
    background: var(--accent);
  }
  .fill.completed {
    background: var(--ok);
  }
  .fill.failed {
    background: var(--danger);
  }
  .bar.indeterminate .fill {
    width: 30% !important;
    animation: slide 1.2s ease-in-out infinite;
  }
  @keyframes slide {
    from {
      left: -30%;
    }
    to {
      left: 100%;
    }
  }
  .pct {
    width: 48px;
    text-align: right;
    color: var(--muted);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }

  .status {
    color: var(--muted);
    white-space: nowrap;
  }
  .status.downloading {
    color: var(--accent);
  }
  .status.completed {
    color: var(--ok);
  }
  .status.failed {
    color: var(--danger);
  }

  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    color: var(--muted);
  }
  .empty p {
    margin: 0;
  }
  .empty b {
    color: var(--text);
  }
  .empty-icon {
    display: grid;
    place-items: center;
    width: 56px;
    height: 56px;
    margin-bottom: 10px;
    border-radius: 14px;
    background: var(--surface-2);
  }

  .banner {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 10px 16px 0;
    padding: 8px 10px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--danger) 12%, transparent);
    color: var(--danger);
    user-select: text;
  }
  .banner span {
    flex: 1;
  }
  .icon-btn {
    display: grid;
    place-items: center;
    border: 0;
    background: transparent;
    color: inherit;
    cursor: pointer;
    padding: 2px;
  }

  .statusbar {
    display: flex;
    gap: 18px;
    padding: 5px 14px;
    border-top: 1px solid var(--border);
    background: var(--surface);
    color: var(--muted);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    min-width: 0;
  }
  .statusbar .speed {
    color: var(--accent);
    font-weight: 600;
  }
  .statusbar .held {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--warn);
  }
  .statusbar .err {
    color: var(--danger);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .hint {
    margin: 0;
    color: var(--muted);
  }
</style>
