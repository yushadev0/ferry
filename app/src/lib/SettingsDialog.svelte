<script lang="ts">
  import { open as pickFolder } from "@tauri-apps/plugin-dialog";
  import * as api from "./api";
  import Icon from "./Icon.svelte";
  import { setLanguage, t } from "./i18n.svelte";

  let {
    settings,
    onsave,
    onclose,
  }: { settings: api.Settings; onsave: (s: api.Settings) => void; onclose: () => void } = $props();

  type Unit = "KB/s" | "MB/s";

  // svelte-ignore state_referenced_locally
  let draft = $state({ ...settings });
  // svelte-ignore state_referenced_locally
  let limitOn = $state(settings.speedLimitKib > 0);
  // Show whole megabytes as MB/s, anything else as KB/s; 5 MB/s when no limit is set yet.
  // svelte-ignore state_referenced_locally
  const kib = settings.speedLimitKib;
  let unit = $state<Unit>(kib === 0 || kib % 1024 === 0 ? "MB/s" : "KB/s");
  let limitValue = $state(kib === 0 ? 5 : kib % 1024 === 0 ? kib / 1024 : kib);
  let error = $state<string | null>(null);
  let busy = $state(false);

  // Preview the theme and language while the dialog is open; closing puts back whatever is saved.
  $effect(() => {
    api.applyTheme(draft.theme);
    setLanguage(draft.language);
    return () => {
      api.applyTheme(settings.theme);
      setLanguage(settings.language);
    };
  });

  async function browse() {
    const picked = await pickFolder({ directory: true, defaultPath: draft.downloadDir || undefined });
    if (typeof picked === "string") draft.downloadDir = picked;
  }

  function changeUnit(next: Unit) {
    if (next === unit) return;
    const n = Number(limitValue) || 0;
    limitValue = next === "MB/s" ? Math.max(1, Math.round(n / 1024)) : n * 1024;
    unit = next;
  }

  async function save(e: SubmitEvent) {
    e.preventDefault();
    if (busy) return;
    busy = true;
    error = null;
    try {
      const n = Number(limitValue) || 0;
      const speedLimitKib = limitOn ? Math.max(1, Math.round(unit === "MB/s" ? n * 1024 : n)) : 0;
      onsave(await api.saveSettings({ ...draft, speedLimitKib }));
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
  <form class="modal wide" aria-label={t("settings.title")} onsubmit={save}>
    <h2>{t("settings.title")}</h2>

    <div class="scroll">
      <section>
        <h3>{t("settings.appearance")}</h3>
        <div class="card">
          <span class="card-icon"><Icon name="theme" size={20} /></span>
          <label class="card-text" for="theme">
            <span>{t("settings.theme")}</span>
            <small>{t("settings.themeHint")}</small>
          </label>
          <select id="theme" bind:value={draft.theme}>
            <option value="system">{t("settings.system")}</option>
            <option value="light">{t("settings.light")}</option>
            <option value="dark">{t("settings.dark")}</option>
          </select>
        </div>
        <div class="card">
          <span class="card-icon"><Icon name="globe" size={20} /></span>
          <label class="card-text" for="language">
            <span>{t("settings.language")}</span>
            <small>{t("settings.languageHint")}</small>
          </label>
          <select id="language" bind:value={draft.language}>
            <option value="system">{t("settings.system")}</option>
            <!-- Each language in its own name, so it can be found in any language. -->
            <option value="en" lang="en">English</option>
            <option value="tr" lang="tr">Türkçe</option>
          </select>
        </div>
      </section>

      <section>
        <h3>{t("settings.downloads")}</h3>
        <div class="card">
          <span class="card-icon"><Icon name="folder" size={20} /></span>
          <div class="card-text">
            <span>{t("settings.folder")}</span>
            <small class="path" title={draft.downloadDir}>{draft.downloadDir || t("settings.notSet")}</small>
          </div>
          <button type="button" class="btn" onclick={browse}>{t("settings.change")}</button>
        </div>

        <div class="card">
          <span class="card-icon"><Icon name="lanes" size={20} /></span>
          <label class="card-text" for="segments">
            <span>{t("settings.segments")}</span>
            <small>{t("settings.segmentsHint")}</small>
          </label>
          <span class="card-value">{draft.segments}</span>
          <input id="segments" type="range" min="1" max="16" bind:value={draft.segments} />
        </div>
      </section>

      <section>
        <h3>{t("settings.queue")}</h3>
        <div class="card">
          <span class="card-icon"><Icon name="stack" size={20} /></span>
          <label class="card-text" for="max-active">
            <span>{t("settings.maxActive")}</span>
            <small>{@html t("settings.maxActiveHint")}</small>
          </label>
          <span class="card-value">{draft.maxActive}</span>
          <input id="max-active" type="range" min="1" max="10" bind:value={draft.maxActive} />
        </div>

        <div class="group">
          <label class="card">
            <span class="card-icon"><Icon name="gauge" size={20} /></span>
            <span class="card-text">
              <span>{t("settings.limit")}</span>
              <small>{t("settings.limitHint")}</small>
            </span>
            <span class="card-value">{limitOn ? t("settings.on") : t("settings.off")}</span>
            <input type="checkbox" role="switch" class="switch" bind:checked={limitOn} />
          </label>
          {#if limitOn}
            <div class="sub">
              <span>{t("settings.limitAtMost")}</span>
              <div class="row">
                <input
                  class="num"
                  type="number"
                  min="1"
                  step={unit === "MB/s" ? 1 : 64}
                  bind:value={limitValue}
                  aria-label={t("settings.limitLabel")} />
                <select value={unit} onchange={(e) => changeUnit(e.currentTarget.value as Unit)} aria-label={t("settings.unit")}>
                  <option>KB/s</option>
                  <option>MB/s</option>
                </select>
              </div>
            </div>
          {/if}
        </div>

        <div class="group">
          <label class="card">
            <span class="card-icon"><Icon name="clock" size={20} /></span>
            <span class="card-text">
              <span>{t("settings.hours")}</span>
              <small>{t("settings.hoursHint")}</small>
            </span>
            <span class="card-value">{draft.scheduleEnabled ? t("settings.on") : t("settings.off")}</span>
            <input type="checkbox" role="switch" class="switch" bind:checked={draft.scheduleEnabled} />
          </label>
          {#if draft.scheduleEnabled}
            <div class="sub">
              <span>{t("settings.from")}</span>
              <div class="row">
                <input type="time" bind:value={draft.scheduleStart} aria-label={t("settings.startTime")} />
                <span class="muted">{t("settings.to")}</span>
                <input type="time" bind:value={draft.scheduleStop} aria-label={t("settings.stopTime")} />
              </div>
            </div>
            {#if draft.scheduleStop < draft.scheduleStart}
              <div class="sub note">{t("settings.overnight")}</div>
            {/if}
          {/if}
        </div>
      </section>

      <section>
        <h3>{t("settings.app")}</h3>
        <label class="card">
          <span class="card-icon"><Icon name="bell" size={20} /></span>
          <span class="card-text">
            <span>{t("settings.notify")}</span>
            <small>{t("settings.notifyHint")}</small>
          </span>
          <span class="card-value">{draft.notifyOnComplete ? t("settings.on") : t("settings.off")}</span>
          <input type="checkbox" role="switch" class="switch" bind:checked={draft.notifyOnComplete} />
        </label>
        <label class="card">
          <span class="card-icon"><Icon name="window" size={20} /></span>
          <span class="card-text">
            <span>{t("settings.tray")}</span>
            <small>{t("settings.trayHint")}</small>
          </span>
          <span class="card-value">{draft.closeToTray ? t("settings.on") : t("settings.off")}</span>
          <input type="checkbox" role="switch" class="switch" bind:checked={draft.closeToTray} />
        </label>
      </section>
    </div>

    {#if error}
      <p class="error-text">{error}</p>
    {/if}

    <div class="actions">
      <button type="button" class="btn" onclick={onclose}>{t("common.cancel")}</button>
      <button type="submit" class="btn primary" disabled={busy}>{t("common.save")}</button>
    </div>
  </form>
</div>

<style>
  .wide {
    width: min(640px, calc(100vw - 32px));
  }
  .scroll {
    display: flex;
    flex-direction: column;
    gap: 20px;
    overflow-y: auto;
    margin: 0 -24px;
    padding: 0 24px 4px;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  h3 {
    margin: 0 0 4px;
    font-size: 13px;
    font-weight: 600;
  }
  label.card:hover {
    background: var(--hover);
  }
  .path {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    user-select: text;
  }
  .card-value {
    min-width: 22px;
    text-align: right;
  }
  input[type="range"] {
    width: 150px;
    flex: none;
  }

  /* A card with extra rows under it, like a Windows 11 expander. */
  .group {
    display: flex;
    flex-direction: column;
  }
  .group > .card:not(:last-child) {
    border-bottom-left-radius: 0;
    border-bottom-right-radius: 0;
  }
  .sub {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    min-height: 52px;
    padding: 8px 16px 8px 52px;
    background: var(--details);
    border: 1px solid var(--border);
    border-top: 0;
  }
  .sub:last-child {
    border-radius: 0 0 6px 6px;
  }
  .sub.note {
    min-height: 0;
    padding-top: 0;
    color: var(--muted);
    font-size: 12px;
    justify-content: flex-end;
  }
  .num {
    width: 96px;
    text-align: right;
  }
  select {
    padding: 6px 8px;
    border: 1px solid var(--field-border);
    border-radius: 6px;
    background: var(--field);
  }
  .muted {
    color: var(--muted);
  }
  p {
    margin: 0;
  }
</style>
