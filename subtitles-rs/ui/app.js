(() => {
  const tauri = window.__TAURI__;
  const listen = tauri?.event?.listen;
  const invoke = tauri?.core?.invoke;
  const getCurrentWindow = tauri?.window?.getCurrentWindow;

  const captionEl = document.getElementById("caption");
  const captionWrap = document.getElementById("caption-wrap");
  const stageEl = document.getElementById("stage");
  const sizeRange = document.getElementById("sizeRange");
  const widthRange = document.getElementById("widthRange");
  const langButtons = Array.from(document.querySelectorAll(".seg-btn"));

  const STORAGE_KEY = "subtitles-ui";
  const defaults = {
    fontSize: 48,
    widthPct: 85,
    outputLanguage: "english",
    controlsHidden: false,
  };

  const state = { ...defaults, ...loadPrefs() };
  if (state.outputLanguage === "original") {
    state.outputLanguage = "chinese";
  }
  let clearTimer = null;

  function loadPrefs() {
    try {
      const raw = window.localStorage.getItem(STORAGE_KEY);
      return raw ? JSON.parse(raw) : {};
    } catch (err) {
      return {};
    }
  }

  function persistPrefs() {
    try {
      window.localStorage.setItem(STORAGE_KEY, JSON.stringify({
        fontSize: state.fontSize,
        widthPct: state.widthPct,
        outputLanguage: state.outputLanguage,
        controlsHidden: state.controlsHidden,
      }));
    } catch (err) {
      // ignore persistence errors
    }
  }

  function applyBodyState() {
    document.body.classList.toggle("controls-hidden", state.controlsHidden);
    document.body.classList.toggle("controls-visible", !state.controlsHidden);
  }

  function setLanguage(lang, shouldInvoke = true) {
    state.outputLanguage = lang;
    langButtons.forEach((btn) => {
      btn.classList.toggle("active", btn.dataset.lang === lang);
    });
    if (shouldInvoke && invoke) {
      invoke("set_output_language", { language: lang }).catch(() => {});
    }
    persistPrefs();
  }

  function updateWidth() {
    const maxWidth = Math.max(280, Math.round(window.innerWidth * (state.widthPct / 100)));
    captionWrap.style.maxWidth = `${maxWidth}px`;
  }

  function fitText() {
    if (!captionEl.textContent) {
      return;
    }

    const max = state.fontSize;
    const min = 18;
    const maxHeight = Math.max(80, stageEl.clientHeight - 8);
    let lo = min;
    let hi = max;
    let best = min;

    for (let i = 0; i < 9; i += 1) {
      const mid = (lo + hi) / 2;
      captionEl.style.fontSize = `${mid}px`;
      if (captionEl.scrollHeight <= maxHeight) {
        best = mid;
        lo = mid;
      } else {
        hi = mid;
      }
    }

    captionEl.style.fontSize = `${best}px`;
  }

  function showIdle() {
    captionEl.textContent = "Listening...";
    captionEl.classList.add("idle");
    captionEl.classList.remove("partial");
    captionEl.style.fontSize = "22px";
  }

  function showCaption(text, isFinal, clear) {
    if (clear || !text || !text.trim()) {
      showIdle();
      return;
    }

    captionEl.textContent = text.trim();
    captionEl.classList.remove("idle");
    captionEl.classList.toggle("partial", !isFinal);

    if (clearTimer) {
      window.clearTimeout(clearTimer);
    }
    clearTimer = window.setTimeout(showIdle, 6000);

    requestAnimationFrame(() => {
      updateWidth();
      fitText();
    });
  }

  function applyInitialState() {
    sizeRange.value = state.fontSize;
    widthRange.value = state.widthPct;
    setLanguage(state.outputLanguage, false);
    applyBodyState();
    updateWidth();
    fitText();
  }

  sizeRange.addEventListener("input", (event) => {
    state.fontSize = Number(event.target.value);
    persistPrefs();
    fitText();
  });

  widthRange.addEventListener("input", (event) => {
    state.widthPct = Number(event.target.value);
    persistPrefs();
    updateWidth();
    fitText();
  });

  langButtons.forEach((btn) => {
    btn.addEventListener("click", () => {
      setLanguage(btn.dataset.lang, true);
    });
  });

  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape") {
      const currentWindow = getCurrentWindow ? getCurrentWindow() : null;
      if (currentWindow?.close) {
        currentWindow.close();
      } else {
        window.close();
      }
    }

    if (event.key.toLowerCase() === "s") {
      state.controlsHidden = !state.controlsHidden;
      applyBodyState();
      persistPrefs();
    }
  });

  window.addEventListener("resize", () => {
    updateWidth();
    fitText();
  });

  if (listen) {
    listen("config", (event) => {
      const cfg = event.payload || {};
      const stored = loadPrefs();
      if (!stored.fontSize && typeof cfg.font_size === "number") {
        state.fontSize = Math.round(cfg.font_size);
      }
      if (!stored.widthPct && typeof cfg.overlay_width_frac === "number") {
        state.widthPct = Math.round(cfg.overlay_width_frac * 100);
      }
      if (!stored.outputLanguage && typeof cfg.output_language === "string") {
        state.outputLanguage =
          cfg.output_language === "original" ? "chinese" : cfg.output_language;
      }
      applyInitialState();
    });

    listen("caption", (event) => {
      const payload = event.payload || {};
      showCaption(payload.text || "", payload.is_final !== false, payload.clear === true);
    });
  }

  applyInitialState();
})();
