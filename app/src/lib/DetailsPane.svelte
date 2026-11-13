<script lang="ts">
  import type { Download } from "./api";
  import FileIcon from "./FileIcon.svelte";
  import Icon from "./Icon.svelte";
  import { t } from "./i18n.svelte";
  import {
    formatBytes,
    formatEta,
    formatSpeed,
    formatStartTime,
    partsOf,
    segmentDone,
    segmentLength,
  } from "./format";

  let {
    d,
    laneSpeeds,
    onshowfolder,
    onopen,
  }: {
    d: Download | null;
    /** Bytes per second by segment start, while downloading. */
    laneSpeeds: Map<number, number>;
    onshowfolder: () => void;
    onopen: () => void;
  } = $props();

  const host = $derived.by(() => {
    try {
      return d ? new URL(d.url).host : "";
    } catch {
      return d?.url ?? "";
    }
  });

  const nameParts = $derived(d ? d.file_name.split(/(?<=[-_.])/) : []);

  const lanes = $derived(
    d && d.status !== "completed"
      ? partsOf(d).map((s, i) => {
          const len = segmentLength(s, d.size);
          const done = segmentDone(s, d.size);
          const speed = laneSpeeds.get(s.start) ?? 0;
          const finished = s.end != null && done >= len;
          return {
            n: i + 1,
            fill: (done / len) * 100,
            finished,
            label: finished
              ? t("details.done")
              : speed
                ? formatSpeed(speed)
                : d.status === "downloading"
                  ? t("details.waiting")
                  : "",
          };
        })
      : [],
  );
</script>

<aside class="pane" aria-label={t("details.label")}>
  {#if !d}
    <p class="empty">{t("details.empty")}</p>
  {:else}
    <div class="head">
      <FileIcon name={d.file_name} size={40} />
      <div class="title">
        <!-- Long names wrap after - _ . rather than mid-word. -->
        <h2>{#each nameParts as part, i (i)}{part}<wbr />{/each}</h2>
        <p class="muted" title={d.url}>{host}</p>
      </div>
    </div>

    <dl class="facts">
      <div>
        <dt>{t("details.downloaded")}</dt>
        <dd>
          {d.status === "completed"
            ? formatBytes(d.size)
            : t("details.ofTotal", { done: formatBytes(d.downloaded), total: formatBytes(d.size) })}
        </dd>
      </div>
      {#if d.status === "downloading"}
        <div>
          <dt>{t("details.speed")}</dt>
          <dd>{formatSpeed(d.speed) || t("details.starting")}</dd>
        </div>
        <div>
          <dt>{t("details.timeLeft")}</dt>
          <dd>{d.size != null ? formatEta(d.size - d.downloaded, d.speed) || "—" : t("details.unknown")}</dd>
        </div>
      {:else if d.status === "scheduled" && d.start_at}
        <div>
          <dt>{t("details.starts")}</dt>
          <dd>{formatStartTime(d.start_at)}</dd>
        </div>
      {/if}
      <div>
        <dt>{t("details.resumable")}</dt>
        <dd>{d.resumable ? t("details.yes") : t("details.no")}</dd>
      </div>
    </dl>

    {#if d.error}
      <p class="error-text">{d.error}</p>
    {/if}

    {#if lanes.length}
      <section>
        <div class="lanes-head">
          <h3>{t("details.connections")}</h3>
          <span class="muted">
            {d.status === "downloading"
              ? t("details.open", { n: d.connections ?? 0 })
              : t("details.parts", { n: lanes.length })}
          </span>
        </div>
        {#if d.resumable}
          <ol class="lanes">
            {#each lanes as l (l.n)}
              <li>
                <span class="n">{l.n}</span>
                <span class="track"><span class="fill" class:finished={l.finished} style:width="{l.fill}%"></span></span>
                <span class="lab">{l.label}</span>
              </li>
            {/each}
          </ol>
        {:else}
          <p class="muted">{t("details.single")}</p>
        {/if}
      </section>
    {/if}

    <div class="actions">
      {#if d.status === "completed"}
        <button class="btn" onclick={onopen}><Icon name="open" /> {t("details.openFile")}</button>
      {/if}
      <button class="btn" onclick={onshowfolder}><Icon name="folder" /> {t("details.showInFolder")}</button>
    </div>
  {/if}
</aside>

<style>
  .pane {
    flex: 0 0 300px;
    min-width: 0;
    border-left: 1px solid var(--divider);
    padding: 20px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    background: var(--details);
    overflow-y: auto;
  }
  .empty {
    margin: auto 0;
    color: var(--muted);
    text-align: center;
  }
  .head {
    display: flex;
    gap: 12px;
    align-items: flex-start;
  }
  .title {
    min-width: 0;
  }
  h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    overflow-wrap: anywhere;
    text-wrap: balance;
    user-select: text;
  }
  .muted {
    margin: 2px 0 0;
    color: var(--muted);
  }
  .facts {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
    margin: 0;
  }
  dt {
    color: var(--muted);
    font-size: 12px;
  }
  dd {
    margin: 0;
    font-variant-numeric: tabular-nums;
  }
  .lanes-head {
    display: flex;
    justify-content: space-between;
    margin-bottom: 8px;
  }
  h3 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
  }
  .lanes {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .lanes li {
    display: grid;
    grid-template-columns: 18px 1fr 70px;
    gap: 8px;
    align-items: center;
    font-size: 12px;
  }
  .n,
  .lab {
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .lab {
    text-align: right;
  }
  .track {
    height: 8px;
    border-radius: 4px;
    background: var(--track);
    overflow: hidden;
  }
  .fill {
    display: block;
    height: 100%;
    background: var(--accent);
    transition: width 0.4s ease;
  }
  .fill.finished {
    background: var(--ok);
  }
  .actions {
    margin-top: auto;
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .actions .btn {
    flex: 1;
    justify-content: center;
  }
  @media (prefers-reduced-motion: reduce) {
    .fill {
      transition: none;
    }
  }
</style>
