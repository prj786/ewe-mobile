<script>
  /**
   * Slider (design/system/components/Slider): the .ewe-slider look with its
   * own pointer and key handling. Drag anywhere on the track; Left/Right move
   * by `step`, PageUp/PageDown by ten steps, Home/End to the ends.
   * `input(v)` reports while dragging, `change(v)` once on release — a seek
   * bar acts on release, a volume slider while it moves.
   */
  let {
    value = 0,
    min = 0,
    max = 100,
    step = 1,
    disabled = false,
    label = "",
    valueText = "",
    large = false,
    input = () => {},
    change = () => {},
    lead,
    trail
  } = $props();

  let track = $state(null);
  let dragging = $state(false);
  let live = $state(0); // the value under the pointer while dragging

  const span = $derived(Math.max(1e-9, max - min));
  const shown = $derived(dragging ? live : Math.min(max, Math.max(min, Number(value) || 0)));
  const pct = $derived(((shown - min) / span) * 100);
  const snap = (v) => Math.min(max, Math.max(min, Math.round(v / step) * step));

  function at(e) {
    const r = track.getBoundingClientRect();
    const f = r.width > 0 ? (e.clientX - r.left) / r.width : 0;
    return snap(min + Math.min(1, Math.max(0, f)) * span);
  }
  function down(e) {
    if (disabled || e.button !== 0) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    dragging = true;
    live = at(e);
    input(live);
  }
  function move(e) {
    if (!dragging) return;
    live = at(e);
    input(live);
  }
  function up() {
    if (!dragging) return;
    dragging = false;
    change(live);
  }
  function key(e) {
    if (disabled) return;
    const big = step * 10;
    let v = shown;
    if (e.key === "ArrowRight" || e.key === "ArrowUp") v += step;
    else if (e.key === "ArrowLeft" || e.key === "ArrowDown") v -= step;
    else if (e.key === "PageUp") v += big;
    else if (e.key === "PageDown") v -= big;
    else if (e.key === "Home") v = min;
    else if (e.key === "End") v = max;
    else return;
    e.preventDefault();
    v = snap(v);
    input(v);
    change(v);
  }
</script>

<div class="ewe-slider {large ? 'ewe-slider--lg' : ''} {disabled ? 'is-disabled' : ''}" data-disabled={disabled ? "" : undefined}>
  {@render lead?.()}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div
    class="slider-hit"
    data-disabled={disabled ? "" : undefined}
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={up}
  >
    <div class="ewe-slider__track" bind:this={track} style="--value: {pct}%">
      <div class="ewe-slider__fill"></div>
      <div
        class="ewe-slider__thumb"
        role="slider"
        tabindex={disabled ? -1 : 0}
        aria-label={label}
        aria-valuemin={min}
        aria-valuemax={max}
        aria-valuenow={Math.round(shown)}
        aria-valuetext={valueText || undefined}
        aria-disabled={disabled}
        onkeydown={key}
      ></div>
    </div>
  </div>
  {@render trail?.()}
</div>
