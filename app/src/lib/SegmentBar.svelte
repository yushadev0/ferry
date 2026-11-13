<script lang="ts">
  import type { Download } from "./api";
  import { partsOf, segmentDone, segmentLength } from "./format";
  import { t } from "./i18n.svelte";

  let { d }: { d: Download } = $props();

  // Each part's width follows its share of the file, so a split shows where it happened.
  const parts = $derived(
    partsOf(d).map((s) => {
      const len = segmentLength(s, d.size);
      return { grow: len, fill: (segmentDone(s, d.size) / len) * 100 };
    }),
  );
</script>

<div
  class="bar {d.status}"
  class:indeterminate={d.size == null && d.status === "downloading"}
  role="progressbar"
  aria-label={t("list.progress")}
  aria-valuemin="0"
  aria-valuemax={d.size ?? undefined}
  aria-valuenow={d.size == null ? undefined : d.downloaded}>
  {#each parts as p, i (i)}
    <span class="part" style:flex-grow={p.grow}><span class="fill" style:width="{p.fill}%"></span></span>
  {/each}
</div>

<style>
  .bar {
    display: flex;
    gap: 2px;
    height: 6px;
    min-width: 0;
  }
  .part {
    flex-basis: 0;
    min-width: 3px;
    border-radius: 2px;
    background: var(--track);
    overflow: hidden;
  }
  .fill {
    display: block;
    height: 100%;
    background: var(--idle);
    transition: width 0.4s ease;
  }
  .downloading .fill {
    background: var(--accent);
  }
  .completed .fill {
    background: var(--ok);
  }
  .failed .fill {
    background: var(--danger);
  }
  .indeterminate .fill {
    width: 30% !important;
    animation: slide 1.2s ease-in-out infinite;
    position: relative;
  }
  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .fill {
      transition: none;
    }
    .indeterminate .fill {
      animation: none;
      width: 100% !important;
      opacity: 0.5;
    }
  }
</style>
