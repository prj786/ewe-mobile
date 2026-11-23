<script>
  /**
   * Media: what is playing on the phone, driven from the desk — play, pause,
   * skip, seek and volume, and which of the phone's players to drive when
   * more than one is open. The position counts on by itself between the
   * phone's reports, so the bar moves like the phone's own.
   */
  import Icon from "../components/ui/Icon.svelte";
  import Empty from "../components/ui/Empty.svelte";
  import Slider from "../components/ui/Slider.svelte";
  import { app, view, mediaDo, mediaSet, mediaPosition } from "../state.svelte.js";
  import { duration } from "../format.js";

  const m = $derived(app.media);
  const on = $derived(view.usable);
  const something = $derived(!!m && (!!m.title || m.players.length > 0));

  // the clock behind the bar, only while something plays
  let now = $state(Date.now());
  $effect(() => {
    if (!m?.playing) {
      now = Date.now();
      return;
    }
    const t = setInterval(() => (now = Date.now()), 500);
    return () => clearInterval(t);
  });
  const position = $derived(m ? mediaPosition(now) : 0);

  let seeking = $state(-1); // where the thumb is while it is held
  let volume = $state(-1); // … and the volume thumb
  const volIcon = (v) => (v <= 0 ? "volumeOff" : v < 50 ? "volumeLow" : "volume");
</script>

<div class="scroll">
  <div class="page page--center">
    {#if !something && !on}
      <Empty icon="music" title="Nothing read yet" desc="What the phone is playing shows when it is in reach." />
    {:else if !something}
      <Empty
        icon="music"
        title="Nothing is playing on {view.device?.name ?? 'the phone'}"
        desc="Start music, a podcast or a video on the phone and control it from here."
      />
    {:else}
      <div class="player">
        {#if m.art}
          <img class="cover cover--lg" src={m.art} alt="" draggable="false" />
        {:else}
          <span class="cover cover--lg cover--none" aria-hidden="true"><Icon name="disc" /></span>
        {/if}

        <div class="player__titles">
          <h1 class="player__title">{m.title || "Nothing playing"}</h1>
          {#if m.artist || m.album}
            <div class="player__by">{[m.artist, m.album].filter(Boolean).join(" · ")}</div>
          {/if}
        </div>

        <div class="player__seek">
          <Slider
            value={position}
            max={Math.max(1, m.length)}
            step={1000}
            disabled={!on || !m.canSeek || m.length <= 0}
            label="Position"
            valueText="{duration(position)} of {duration(m.length)}"
            input={(v) => (seeking = v)}
            change={(v) => {
              seeking = -1;
              mediaSet("position", Math.round(v));
            }}
          />
          <div class="player__times">
            <span>{duration(seeking >= 0 ? seeking : position)}</span>
            <span>{m.length > 0 ? duration(m.length) : "Live"}</span>
          </div>
        </div>

        <div class="player__buttons">
          <button type="button" class="ewe-iconbtn ewe-iconbtn--ghost ewe-iconbtn--xl" title="Previous" aria-label="Previous" disabled={!on} onclick={() => mediaDo("Previous")}>
            <Icon name="previous" />
          </button>
          <button
            type="button"
            class="ewe-iconbtn ewe-iconbtn--primary ewe-iconbtn--2xl ewe-iconbtn--round"
            title={m.playing ? "Pause" : "Play"}
            aria-label={m.playing ? "Pause" : "Play"}
            disabled={!on}
            onclick={() => mediaDo("PlayPause")}
          >
            <Icon name={m.playing ? "pause" : "play"} />
          </button>
          <button type="button" class="ewe-iconbtn ewe-iconbtn--ghost ewe-iconbtn--xl" title="Next" aria-label="Next" disabled={!on} onclick={() => mediaDo("Next")}>
            <Icon name="next" />
          </button>
        </div>

        <div class="player__volume">
          <Slider
            value={volume >= 0 ? volume : m.volume}
            disabled={!on}
            label="Volume on the phone"
            valueText="{volume >= 0 ? volume : m.volume}%"
            input={(v) => {
              volume = v;
            }}
            change={(v) => {
              volume = -1;
              mediaSet("volume", Math.round(v));
            }}
          >
            {#snippet lead()}<Icon name={volIcon(volume >= 0 ? volume : m.volume)} />{/snippet}
            {#snippet trail()}<span class="ewe-slider__value">{volume >= 0 ? volume : m.volume}%</span>{/snippet}
          </Slider>
        </div>

        {#if m.players.length > 1}
          <div class="ewe-seg player__players" role="radiogroup" aria-label="Player">
            {#each m.players as p (p)}
              <button
                type="button"
                role="radio"
                aria-checked={m.player === p}
                class="ewe-seg__item"
                class:is-selected={m.player === p}
                disabled={!on}
                onclick={() => mediaSet("player", p)}
              >
                {p}
              </button>
            {/each}
          </div>
        {:else if m.player}
          <div class="player__source">Playing in {m.player}</div>
        {/if}
      </div>
    {/if}
  </div>
</div>
