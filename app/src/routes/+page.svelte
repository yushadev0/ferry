<script lang="ts">
  import { onMount } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import { Menu, PredefinedMenuItem } from "@tauri-apps/api/menu";
  import * as api from "$lib/api";
  import { formatBytes, formatEta, formatPercent, formatSpeed, formatStartTime, percent } from "$lib/format";
  import Icon, { type IconName } from "$lib/Icon.svelte";
  import AppLogo from "$lib/AppLogo.svelte";
  import FileIcon from "$lib/FileIcon.svelte";
  import SegmentBar from "$lib/SegmentBar.svelte";
  import DetailsPane from "$lib/DetailsPane.svelte";
  import AddDialog from "$lib/AddDialog.svelte";
  import SettingsDialog from "$lib/SettingsDialog.svelte";
  import ScheduleDialog from "$lib/ScheduleDialog.svelte";
  import { setLanguage, t } from "$lib/i18n.svelte";
  import { startUpdateChecks } from "$lib/updates.svelte";
  import type { Key } from "$lib/locales/en";

  type Filter = "all" | "active" | "completed" | "failed";

  const FILTERS: { id: Filter; label: Key; icon: IconName }[] = [
    { id: "all", label: "nav.all", icon: "list" },
    { id: "active", label: "nav.active", icon: "activity" },
    { id: "completed", label: "nav.completed", icon: "check" },
    { id: "failed", label: "nav.failed", icon: "alert" },
  ];

  const UNFINISHED: api.Status[] = ["queued", "scheduled", "downloading", "paused"];

  let downloads = $state<api.Download[]>([]);
  let settings = $state<api.Settings | null>(null);
  let queue = $state<api.QueueState>({ running: true, resumesAt: null });
  let filter = $state<Filter>("all");
  let search = $state("");
  const selected = new SvelteSet<number>();
  let lastClicked: number | null = null;

  /** Per-download, per-segment speeds (bytes/s by segment start). */
  let laneSpeeds = $state<Record<number, Map<number, number>>>({});
  const lastSample = new Map<number, { at: number; pos: Map<number, number> }>();

  let showAdd = $state(false);
  let addUrl = $state("");
  /** Browser downloads waiting for the Add dialog, oldest first. */
  let captures = $state<api.Capture[]>([]);
  let capture = $state<api.Capture | null>(null);
  let showSettings = $state(false);
  let scheduleIds = $state<number[] | null>(null);
  let confirmRemove = $state(false);
  let deleteFiles = $state(false);
  let banner = $state<string | null>(null);
  let searchInput: HTMLInputElement | undefined = $state();

  const matches = (d: api.Download, f: Filter) =>
    f === "all" ||
    (f === "active" && UNFINISHED.includes(d.status)) ||
    (f === "completed" && d.status === "completed") ||
    (f === "failed" && d.status === "failed");

  const needle = $derived(search.trim().toLowerCase());
  const visible = $derived(
    downloads
      .filter((d) => matches(d, filter) && (!needle || d.file_name.toLowerCase().includes(needle)))
      .sort((a, b) => b.id - a.id),
  );
  const counts = $derived(
    Object.fromEntries(FILTERS.map((f) => [f.id, downloads.filter((d) => matches(d, f.id)).length])),
  );
  const selection = $derived(downloads.filter((d) => selected.has(d.id)));
  const focused = $derived(selection.find((d) => d.id === lastClicked) ?? selection[0] ?? null);

  const running = $derived(downloads.filter((d) => d.status === "downloading"));
  const waiting = $derived(downloads.filter((d) => d.status === "queued"));
  const scheduled = $derived(
    downloads.filter((d) => d.status === "scheduled").sort((a, b) => (a.start_at ?? 0) - (b.start_at ?? 0)),
  );
  const totalSpeed = $derived(running.reduce((sum, d) => sum + (d.speed ?? 0), 0));

  const can = $derived({
    resume: selection.some((d) => ["paused", "failed", "scheduled"].includes(d.status)),
    pause: selection.some((d) => ["downloading", "queued", "scheduled"].includes(d.status)),
    startNow: selection.some((d) => d.status !== "completed" && d.status !== "downloading"),
    schedule: selection.some((d) => d.status !== "completed"),
    remove: selection.length > 0,
  });

  function statusLine(d: api.Download): { icon: IconName; text: string; tone: string } {
    switch (d.status) {
      case "downloading":
        return {
          icon: "down",
          text: d.connections ? t("status.connections", { n: d.connections }) : t("status.downloading"),
          tone: "accent",
        };
      case "queued":
        return {
          icon: "clock",
          text: queue.running ? t("status.waiting") : t("status.waitingResumes", { time: queue.resumesAt ?? "" }),
          tone: "muted",
        };
      case "scheduled":
        return {
          icon: "clock",
          text: d.start_at ? t("status.starts", { time: formatStartTime(d.start_at, new Date(), true) }) : t("status.scheduled"),
          tone: "muted",
        };
      case "paused":
        return { icon: "pause", text: t("status.paused"), tone: "warn" };
      case "failed":
        return { icon: "alert", text: d.error ?? t("status.failed"), tone: "danger" };
      case "completed":
        return { icon: "check", text: t("status.finished"), tone: "ok" };
    }
  }

  async function run(action: () => Promise<unknown>) {
    try {
      banner = null;
      await action();
    } catch (e) {
      banner = api.message(e);
    }
  }

  const find = (id: number) => downloads.find((d) => d.id === id);
  const ids = () => [...selected];

  function trackLanes(id: number, segments: api.Segment[]) {
    const now = performance.now();
    const prev = lastSample.get(id);
    const pos = new Map(segments.map((s) => [s.start, s.pos]));
    lastSample.set(id, { at: now, pos });
    if (!prev) return;
    const dt = (now - prev.at) / 1000;
    if (dt <= 0) return;
    const old = laneSpeeds[id] ?? new Map<number, number>();
    const next = new Map<number, number>();
    for (const [start, p] of pos) {
      const before = prev.pos.get(start);
      const instant = before == null ? 0 : Math.max(0, (p - before) / dt);
      // Smooth a little so the numbers don't flicker.
      next.set(start, 0.5 * instant + 0.5 * (old.get(start) ?? instant));
    }
    laneSpeeds[id] = next;
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
        if (ev.status !== "downloading") {
          d.speed = 0;
          d.connections = 0;
          lastSample.delete(ev.id);
          delete laneSpeeds[ev.id];
        }
        if (ev.status === "completed" || ev.status === "scheduled" || ev.status === "queued") {
          Object.assign(d, await api.getDownload(ev.id), { speed: 0, connections: 0 });
        }
        break;
      }
      case "progress": {
        const d = find(ev.id);
        if (!d) break;
        d.downloaded = ev.downloaded;
        d.size = ev.total ?? d.size;
        d.segments = ev.segments;
        d.speed = d.status === "downloading" ? ev.speed : 0;
        d.connections = ev.connections;
        if (d.status === "downloading") trackLanes(ev.id, ev.segments);
        break;
      }
      case "removed":
        downloads = downloads.filter((d) => d.id !== ev.id);
        selected.delete(ev.id);
        break;
    }
  }

  $effect(() => {
    if (settings) {
      api.applyTheme(settings.theme);
      setLanguage(settings.language);
    }
  });

  onMount(() => {
    let disposed = false;
    const unlisten: (() => void)[] = [];
    (async () => {
      unlisten.push(await api.onEngineEvent(handleEvent));
      unlisten.push(await api.onQueueState((s) => (queue = s)));
      unlisten.push(await api.onCapture(pullCaptures));
      if (disposed) unlisten.forEach((u) => u());
      [downloads, settings, queue] = await Promise.all([api.listDownloads(), api.getSettings(), api.queueState()]);
      // Downloads the browser sent while the window was still loading.
      await pullCaptures();
      startUpdateChecks(() => settings?.autoUpdateCheck ?? true);
    })().catch((e) => (banner = api.message(e)));
    return () => {
      disposed = true;
      unlisten.forEach((u) => u());
    };
  });

  function select(e: MouseEvent, id: number) {
    if (e.shiftKey && lastClicked !== null) {
      const order = visible.map((d) => d.id);
      const [a, b] = [order.indexOf(lastClicked), order.indexOf(id)].sort((x, y) => x - y);
      if (a >= 0) {
        if (!e.ctrlKey && !e.metaKey) selected.clear();
        order.slice(a, b + 1).forEach((i) => selected.add(i));
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

  async function contextMenu(e: MouseEvent, d: api.Download) {
    e.preventDefault();
    if (!selected.has(d.id)) {
      selected.clear();
      selected.add(d.id);
      lastClicked = d.id;
    }
    const sep = () => PredefinedMenuItem.new({ item: "Separator" });
    const menu = await Menu.new({
      items: [
        { id: "open", text: t("menu.open"), enabled: d.status === "completed", action: () => run(() => api.openFile(d.id)) },
        { id: "folder", text: t("menu.showInFolder"), action: () => run(() => api.showInFolder(d.id)) },
        await sep(),
        { id: "resume", text: t("menu.resume"), enabled: can.resume, action: () => run(() => api.startDownloads(ids())) },
        { id: "pause", text: t("menu.pause"), enabled: can.pause, action: () => run(() => api.pauseDownloads(ids())) },
        { id: "now", text: t("menu.startNow"), enabled: can.startNow, action: () => run(() => api.startNow(ids())) },
        { id: "schedule", text: t("menu.schedule"), enabled: can.schedule, action: () => (scheduleIds = ids()) },
        await sep(),
        { id: "copy", text: t("menu.copyLink"), action: () => run(() => navigator.clipboard.writeText(d.url)) },
        { id: "remove", text: t("menu.remove"), action: () => (confirmRemove = true) },
      ],
    });
    await menu.popup();
  }

  function openAdd(url = "") {
    addUrl = url;
    capture = null;
    showAdd = true;
  }

  async function pullCaptures() {
    captures.push(...(await api.takeCaptures()));
  }

  // Show the next browser download as soon as no other dialog is open.
  $effect(() => {
    if (captures.length === 0 || !settings || dialogOpen()) return;
    capture = captures.shift() ?? null;
    addUrl = "";
    showAdd = true;
  });

  function closeAdd() {
    showAdd = false;
    capture = null;
  }

  const dialogOpen = () => showAdd || showSettings || confirmRemove || scheduleIds !== null;

  function onPaste(e: ClipboardEvent) {
    const target = e.target as HTMLElement;
    if (dialogOpen() || target.closest("input")) return;
    const text = e.clipboardData?.getData("text")?.trim() ?? "";
    if (/^(https?|ftp):\/\/\S+$/i.test(text)) openAdd(text);
  }

  function onkeydown(e: KeyboardEvent) {
    if (dialogOpen()) return;
    const mod = e.ctrlKey || e.metaKey;
    const inInput = (e.target as HTMLElement).closest("input");
    if (mod && e.key.toLowerCase() === "n") {
      e.preventDefault();
      openAdd();
    } else if (mod && e.key.toLowerCase() === "f") {
      e.preventDefault();
      searchInput?.focus();
    } else if (inInput) {
      if (e.key === "Escape") (e.target as HTMLInputElement).blur();
    } else if (mod && e.key.toLowerCase() === "a") {
      e.preventDefault();
      visible.forEach((d) => selected.add(d.id));
    } else if (e.key === "Delete" && selected.size) {
      confirmRemove = true;
    } else if (e.key === "Enter" && focused) {
      activate(focused);
    }
  }

  async function removeSelected() {
    const list = ids();
    confirmRemove = false;
    await run(() => api.removeDownloads(list, deleteFiles));
    deleteFiles = false;
  }
</script>

<svelte:window {onkeydown} onpaste={onPaste} />

<div class="app">
  <nav class="nav" aria-label={t("nav.categories")}>
    <div class="brand">
      <AppLogo size={22} />
      <span>Ferry</span>
    </div>

    {#each FILTERS as f (f.id)}
      <button class="nav-item" class:active={filter === f.id} onclick={() => (filter = f.id)} aria-current={filter === f.id}>
        <span class="nav-icon"><Icon name={f.icon} /></span>
        <span class="nav-label">{t(f.label)}</span>
        <span class="nav-count">{counts[f.id]}</span>
      </button>
    {/each}

    <h2 class="nav-heading">{t("nav.queue")}</h2>
    <dl class="queue-card">
      <div><dt>{t("queue.running")}</dt><dd>{t("queue.runningOf", { n: running.length, max: settings?.maxActive ?? "–" })}</dd></div>
      <div><dt>{t("queue.waiting")}</dt><dd>{waiting.length}</dd></div>
      <div>
        <dt>{t("queue.nextScheduled")}</dt>
        <dd>{scheduled.length ? formatStartTime(scheduled[0].start_at ?? 0) : t("queue.none")}</dd>
      </div>
      {#if !queue.running}
        <div class="held"><dt>{t("queue.onHold")}</dt><dd>{t("queue.until", { time: queue.resumesAt ?? "" })}</dd></div>
      {/if}
    </dl>

    <div class="nav-spacer"></div>
    <button class="nav-item" disabled={!settings} onclick={() => (showSettings = true)}>
      <span class="nav-icon"><Icon name="settings" /></span>
      <span class="nav-label">{t("nav.settings")}</span>
    </button>
  </nav>

  <main class="main">
    <div class="commands" role="toolbar" aria-label={t("cmd.toolbar")}>
      <button class="btn primary" onclick={() => openAdd()} title={t("cmd.addTitle")}>
        <Icon name="plus" stroke={2.2} /> {t("cmd.add")}
      </button>
      <span class="sep"></span>
      <button class="cmd" disabled={!can.resume} onclick={() => run(() => api.startDownloads(ids()))} title={t("cmd.resumeTitle")}>
        <Icon name="play" /> {t("cmd.resume")}
      </button>
      <button class="cmd" disabled={!can.pause} onclick={() => run(() => api.pauseDownloads(ids()))}>
        <Icon name="pause" /> {t("cmd.pause")}
      </button>
      <button class="cmd" disabled={!can.startNow} onclick={() => run(() => api.startNow(ids()))} title={t("cmd.startNowTitle")}>
        <Icon name="zap" /> {t("cmd.startNow")}
      </button>
      <button class="cmd" disabled={!can.remove} onclick={() => (confirmRemove = true)} title={t("cmd.removeTitle")}>
        <Icon name="trash" /> {t("cmd.remove")}
      </button>
      <span class="sep"></span>
      <button class="cmd" disabled={!can.schedule} onclick={() => (scheduleIds = ids())} title={t("cmd.scheduleTitle")}>
        <Icon name="clock" /> {t("cmd.schedule")}
      </button>
      <span class="spacer"></span>
      <label class="search">
        <Icon name="search" size={14} />
        <input type="search" placeholder={t("cmd.search")} aria-label={t("cmd.searchLabel")} bind:value={search} bind:this={searchInput} />
      </label>
    </div>

    {#if banner}
      <div class="banner" role="alert">
        <Icon name="alert" />
        <span>{banner}</span>
        <button class="icon-btn" aria-label={t("cmd.dismiss")} onclick={() => (banner = null)}><Icon name="x" size={14} /></button>
      </div>
    {/if}

    <div class="content">
      <div class="table-wrap">
        <div class="table">
          <div class="thead">
            <span>{t("list.name")}</span><span class="num">{t("list.size")}</span><span>{t("list.progress")}</span><span class="num">{t("list.speed")}</span><span class="num">{t("list.left")}</span>
          </div>

          {#if visible.length === 0}
            <div class="empty">
              <AppLogo size={48} />
              {#if needle}
                <p>{t("list.noMatch", { q: search.trim() })}</p>
              {:else if downloads.length === 0}
                <p><b>{t("list.emptyTitle")}</b></p>
                <p>{@html t("list.emptyHint")}</p>
              {:else}
                <p>{t("list.emptyCategory")}</p>
              {/if}
            </div>
          {:else}
            <div class="rows" role="listbox" aria-label={t("list.downloads")} aria-multiselectable="true" tabindex="-1">
              {#each visible as d (d.id)}
                {@const st = statusLine(d)}
                {@const pct = percent(d.downloaded, d.size)}
                <div
                  class="tr"
                  class:selected={selected.has(d.id)}
                  role="option"
                  tabindex="-1"
                  aria-selected={selected.has(d.id)}
                  onclick={(e) => select(e, d.id)}
                  ondblclick={() => activate(d)}
                  oncontextmenu={(e) => contextMenu(e, d)}
                  onkeydown={() => {}}>
                  <div class="name" title={`${d.file_name}\n${d.url}`}>
                    <FileIcon name={d.file_name} />
                    <div class="name-text">
                      <div class="fname">{d.file_name}</div>
                      <div class="status {st.tone}"><Icon name={st.icon} size={12} stroke={2.4} />{st.text}</div>
                    </div>
                  </div>
                  <span class="num">{formatBytes(d.size)}</span>
                  <div class="progress">
                    <SegmentBar {d} />
                    <span class="pct">{pct === null ? formatBytes(d.downloaded) : formatPercent(pct)}</span>
                  </div>
                  <span class="num">{d.status === "downloading" ? formatSpeed(d.speed) : ""}</span>
                  <span class="num muted">
                    {d.status === "downloading" && d.size != null ? formatEta(d.size - d.downloaded, d.speed) : ""}
                  </span>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      </div>

      <DetailsPane
        d={focused}
        laneSpeeds={focused ? (laneSpeeds[focused.id] ?? new Map()) : new Map()}
        onopen={() => focused && run(() => api.openFile(focused.id))}
        onshowfolder={() => focused && run(() => api.showInFolder(focused.id))} />
    </div>

    <footer class="statusbar">
      <span>{t("bar.downloading", { n: running.length })}</span>
      {#if waiting.length}<span>{t("bar.waiting", { n: waiting.length })}</span>{/if}
      {#if scheduled.length}<span>{t("bar.scheduled", { n: scheduled.length })}</span>{/if}
      {#if totalSpeed}<span class="strong">{t("bar.total", { speed: formatSpeed(totalSpeed) })}</span>{/if}
      <span>{settings?.speedLimitKib ? t("bar.limit", { speed: formatSpeed(settings.speedLimitKib * 1024) }) : t("bar.noLimit")}</span>
      {#if !queue.running}
        <span class="held"><Icon name="clock" size={12} /> {t("bar.onHold", { time: queue.resumesAt ?? "" })}</span>
      {/if}
    </footer>
  </main>
</div>

{#if showAdd && settings}
  {#key capture}
    <AddDialog defaultDir={settings.downloadDir} segments={settings.segments} initialUrl={addUrl} {capture} onclose={closeAdd} />
  {/key}
{/if}

{#if showSettings && settings}
  <SettingsDialog {settings} onsave={(s) => (settings = s)} onclose={() => (showSettings = false)} />
{/if}

{#if scheduleIds}
  <ScheduleDialog ids={scheduleIds} defaultTime={settings?.scheduleStart ?? "02:00"} onclose={() => (scheduleIds = null)} />
{/if}

{#if confirmRemove}
  <div class="backdrop" role="presentation" onclick={(e) => e.target === e.currentTarget && (confirmRemove = false)}>
    <div class="modal narrow" role="alertdialog" aria-label={t("remove.label")}>
      <h2>{selected.size === 1 ? t("remove.titleOne") : t("remove.titleMany", { n: selected.size })}</h2>
      <p class="hint">{t("remove.hint")}</p>
      <label class="check">
        <input type="checkbox" bind:checked={deleteFiles} />
        {t("remove.deleteFiles")}
      </label>
      <div class="actions">
        <button class="btn" onclick={() => (confirmRemove = false)}>{t("common.cancel")}</button>
        <button class="btn primary" onclick={removeSelected}>{t("remove.confirm")}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .app {
    display: flex;
    height: 100vh;
    min-width: 0;
  }

  /* Navigation pane */
  .nav {
    flex: 0 0 228px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 12px 8px 10px;
    overflow-y: auto;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 12px 14px;
    font-size: 14px;
    font-weight: 600;
  }
  .nav-item {
    position: relative;
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 36px;
    padding: 8px 12px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    text-align: left;
  }
  .nav-item:hover:not(:disabled) {
    background: var(--hover);
  }
  .nav-item.active {
    background: var(--nav-active);
    box-shadow: var(--nav-shadow);
    font-weight: 600;
  }
  .nav-item.active::before {
    content: "";
    position: absolute;
    left: 0;
    top: 10px;
    bottom: 10px;
    width: 3px;
    border-radius: 2px;
    background: var(--accent);
  }
  .nav-icon {
    display: flex;
    color: var(--muted);
  }
  .nav-item.active .nav-icon {
    color: var(--accent);
  }
  .nav-label {
    flex: 1;
  }
  .nav-count {
    color: var(--muted);
    font-weight: 400;
    font-variant-numeric: tabular-nums;
  }
  .nav-heading {
    margin: 18px 12px 6px;
    color: var(--muted);
    font-size: 12px;
    font-weight: 400;
  }
  .queue-card {
    margin: 0 4px;
    padding: 10px 12px;
    background: var(--panel);
    border-radius: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .queue-card div {
    display: flex;
    justify-content: space-between;
    gap: 8px;
  }
  .queue-card dd {
    margin: 0;
    font-variant-numeric: tabular-nums;
    text-align: right;
  }
  .queue-card .held {
    color: var(--warn);
  }
  .nav-spacer {
    flex: 1;
    min-height: 16px;
  }

  /* Main panel */
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    margin-top: 8px;
    background: var(--panel);
    border: 1px solid var(--border);
    border-right: 0;
    border-bottom: 0;
    border-top-left-radius: 8px;
  }
  .commands {
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--divider);
    flex-wrap: wrap;
  }
  .sep {
    width: 1px;
    height: 22px;
    background: var(--border);
    margin: 0 8px;
  }
  .spacer {
    flex: 1;
  }
  .cmd {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    min-height: 32px;
    padding: 6px 10px;
    border: 0;
    border-radius: 6px;
    background: transparent;
  }
  .cmd:hover:not(:disabled) {
    background: var(--hover);
  }
  .cmd:active:not(:disabled) {
    background: var(--pressed);
  }
  .cmd:disabled {
    color: var(--idle);
  }
  .search {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 0 1 240px;
    min-width: 130px;
    padding: 0 10px;
    border: 1px solid var(--field-border);
    border-bottom-color: var(--field-underline);
    border-radius: 6px;
    background: var(--field);
    color: var(--muted);
  }
  .search input {
    flex: 1;
    min-width: 0;
    padding: 6px 0;
    border: 0;
    background: transparent;
    color: var(--text);
    outline: none;
  }
  .search:focus-within {
    border-bottom: 2px solid var(--accent);
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
    padding: 4px;
    border-radius: 4px;
  }

  .content {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .table-wrap {
    flex: 1;
    min-width: 0;
    overflow-x: auto;
    display: flex;
  }
  .table {
    flex: 1;
    min-width: 560px;
    display: flex;
    flex-direction: column;
  }
  .thead,
  .tr {
    display: grid;
    grid-template-columns: minmax(150px, 1fr) 72px minmax(110px, 200px) 84px 76px;
    gap: 14px;
    align-items: center;
    padding: 0 20px;
  }
  .thead {
    height: 36px;
    color: var(--muted);
    font-size: 12px;
    border-bottom: 1px solid var(--divider);
  }
  .rows {
    flex: 1;
    overflow-y: auto;
    outline: none;
  }
  .tr {
    height: 50px;
    border-bottom: 1px solid var(--row-line);
  }
  .tr:hover {
    background: var(--hover);
  }
  .tr.selected {
    background: var(--selected);
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .muted {
    color: var(--muted);
  }
  .name {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }
  .name-text {
    min-width: 0;
  }
  .fname,
  .status {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
  }
  .status.accent {
    color: var(--accent);
  }
  .status.muted {
    color: var(--muted);
  }
  .status.warn {
    color: var(--warn);
  }
  .status.danger {
    color: var(--danger);
  }
  .status.ok {
    color: var(--ok);
  }
  .progress {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .progress :global(.bar) {
    flex: 1;
  }
  .pct {
    width: 40px;
    flex: none;
    text-align: right;
    font-size: 12px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    color: var(--muted);
  }
  .empty p {
    margin: 0;
  }
  .empty :global(svg) {
    margin-bottom: 10px;
    opacity: 0.9;
  }
  .empty b {
    color: var(--text);
  }

  .statusbar {
    display: flex;
    gap: 20px;
    padding: 7px 20px;
    border-top: 1px solid var(--divider);
    color: var(--muted);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
    flex-wrap: wrap;
  }
  .statusbar .strong {
    color: var(--text);
    font-weight: 600;
  }
  .statusbar .held {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    color: var(--warn);
  }

  .narrow {
    width: min(420px, calc(100vw - 32px));
  }
  .hint {
    margin: 0;
    color: var(--muted);
  }

  /* Narrow windows: hide the details pane, then shrink the navigation. */
  @media (max-width: 1080px) {
    .content :global(.pane) {
      display: none;
    }
  }
  @media (max-width: 760px) {
    .nav {
      flex-basis: 64px;
    }
    .nav-label,
    .nav-count,
    .nav-heading,
    .queue-card,
    .brand span {
      display: none;
    }
  }
</style>
