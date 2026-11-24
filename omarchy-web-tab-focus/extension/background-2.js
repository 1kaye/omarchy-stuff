// Keep this filename versioned when changing code so Chromium refreshes the
// cached service worker for --load-extension unpacked extensions.

const HOST_NAME = "com.user.omarchy_web_tab_focus";
const KEEPALIVE_ALARM = "omarchy-web-tab-focus-keepalive";
const STARTED_AT = Date.now();

let port = null;
let reconnectDelayMs = 1000;
let reconnectTimer = null;

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function normalizeHost(host) {
  return host.toLowerCase().replace(/^www\./, "");
}

function isBlankNewTabUrl(url) {
  if (!url) return true;
  const lower = url.toLowerCase();
  return (
    lower === "about:blank" ||
    lower === "about:newtab" ||
    lower.startsWith("chrome://newtab") ||
    lower.startsWith("chrome://new-tab-page") ||
    lower.startsWith("brave://newtab") ||
    lower.startsWith("edge://newtab") ||
    lower.startsWith("vivaldi://newtab")
  );
}

function urlMatches(tabUrl, pattern) {
  if (!tabUrl || !pattern) return false;
  try {
    const parsed = new URL(tabUrl);
    if (parsed.protocol !== "http:" && parsed.protocol !== "https:") return false;

    const slashIdx = pattern.indexOf("/");
    if (slashIdx === -1) {
      const targetHost = normalizeHost(pattern);
      const tabHost = normalizeHost(parsed.hostname);
      return tabHost === targetHost || tabHost.endsWith("." + targetHost);
    }

    const targetHost = normalizeHost(pattern.slice(0, slashIdx));
    const targetPath = pattern.slice(slashIdx);
    const tabHost = normalizeHost(parsed.hostname);
    const hostOk = tabHost === targetHost || tabHost.endsWith("." + targetHost);
    return hostOk && (parsed.pathname + parsed.search + parsed.hash).startsWith(targetPath);
  } catch {
    return false;
  }
}

async function findMatchingTab(match) {
  const tabs = await chrome.tabs.query({});
  const candidates = tabs.filter((t) => urlMatches(t.url || t.pendingUrl, match));
  if (candidates.length === 0) {
    return { target: null, tabs };
  }

  const windows = await chrome.windows.getAll({});
  const winTypes = new Map(windows.map((w) => [w.id, w.type]));

  candidates.sort((a, b) => {
    const aNorm = winTypes.get(a.windowId) === "normal" ? 1 : 0;
    const bNorm = winTypes.get(b.windowId) === "normal" ? 1 : 0;
    if (aNorm !== bNorm) return bNorm - aNorm;
    const aAcc = a.lastAccessed || 0;
    const bAcc = b.lastAccessed || 0;
    if (aAcc !== bAcc) return bAcc - aAcc;
    return (b.active ? 1 : 0) - (a.active ? 1 : 0);
  });

  return { target: candidates[0], tabs };
}

/**
 * During cold browser start / session restore ("Continue where you left off"),
 * Chromium may connect the extension slightly before all restored tabs are populated.
 * Poll briefly so we immediately reuse a restored matching tab instead of duplicating it.
 */
async function findMatchingTabWithRestoreWait(match) {
  const deadline = Date.now() + 3000;
  let firstTabsAt = null;
  let lastCount = -1;
  let lastCountChangeAt = Date.now();

  while (true) {
    const { target, tabs } = await findMatchingTab(match);
    if (target) {
      return target;
    }

    const now = Date.now();
    if (tabs.length > 0 && firstTabsAt === null) {
      firstTabsAt = now;
    }
    if (tabs.length !== lastCount) {
      lastCount = tabs.length;
      lastCountChangeAt = now;
    }

    const hasUnpopulatedTab = tabs.some((t) => !(t.url || t.pendingUrl));
    const settled =
      tabs.length > 0 &&
      !hasUnpopulatedTab &&
      now - lastCountChangeAt >= 700 &&
      now - STARTED_AT >= 1200 &&
      (firstTabsAt === null || now - firstTabsAt >= 800);

    if (settled || now >= deadline) {
      return null;
    }
    await sleep(100);
  }
}

async function handleRequest(msg) {
  const { id, action, url, match, wait_for_restore } = msg || {};

  if (action === "ping") {
    return { id, ok: true, pong: true };
  }

  if (action === "focus_or_open") {
    const target = wait_for_restore
      ? await findMatchingTabWithRestoreWait(match)
      : (await findMatchingTab(match)).target;

    if (target) {
      await chrome.tabs.update(target.id, { active: true });
      await chrome.windows.update(target.windowId, { focused: true });
      const updated = await chrome.tabs.get(target.id).catch(() => target);
      return {
        id,
        ok: true,
        found: true,
        created_window: false,
        tab_id: target.id,
        window_id: target.windowId,
        title: updated.title || target.title || "",
        prev_title: "",
        url: updated.url || target.url || "",
      };
    }
  }

  if (action === "focus_or_open" || action === "open") {
    let targetWin = null;
    try {
      targetWin = await chrome.windows.getLastFocused({ windowTypes: ["normal"] });
    } catch {
      targetWin = null;
    }

    if (!targetWin || targetWin.type !== "normal") {
      const allNormal = await chrome.windows.getAll({ windowTypes: ["normal"] });
      targetWin = allNormal[0] || null;
    }

    if (targetWin) {
      const winTabs = await chrome.tabs.query({ windowId: targetWin.id });
      const activeTab = winTabs.find((t) => t.active);
      const prevTitle = activeTab?.title || "";

      // On a fresh empty window with a single blank New Tab, navigate it in place
      // instead of leaving an orphan New Tab next to the requested URL.
      if (
        action === "focus_or_open" &&
        winTabs.length === 1 &&
        isBlankNewTabUrl(winTabs[0].url || winTabs[0].pendingUrl)
      ) {
        const reused = await chrome.tabs.update(winTabs[0].id, { url, active: true });
        await chrome.windows.update(targetWin.id, { focused: true });
        return {
          id,
          ok: true,
          found: false,
          created_window: false,
          tab_id: reused.id,
          window_id: targetWin.id,
          title: reused.title || "",
          prev_title: prevTitle,
          url: reused.pendingUrl || reused.url || url,
        };
      }

      const created = await chrome.tabs.create({ windowId: targetWin.id, url, active: true });
      await chrome.windows.update(targetWin.id, { focused: true });
      return {
        id,
        ok: true,
        found: false,
        created_window: false,
        tab_id: created.id,
        window_id: targetWin.id,
        title: created.title || "",
        prev_title: prevTitle,
        url: created.pendingUrl || created.url || url,
      };
    }

    const createdWin = await chrome.windows.create({ url, focused: true });
    return {
      id,
      ok: true,
      found: false,
      created_window: true,
      window_id: createdWin.id,
      title: "",
      prev_title: "",
      url,
    };
  }

  return { id, ok: false, error: "unknown action: " + String(action) };
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  const delay = reconnectDelayMs;
  reconnectDelayMs = Math.min(reconnectDelayMs * 2, 10000);
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connect();
  }, delay);
}

function connect() {
  if (port) return;
  try {
    const nextPort = chrome.runtime.connectNative(HOST_NAME);
    port = nextPort;
    reconnectDelayMs = 1000;

    nextPort.onMessage.addListener((msg) => {
      handleRequest(msg)
        .then((resp) => {
          if (port === nextPort) {
            nextPort.postMessage(resp);
          }
        })
        .catch((err) => {
          if (port === nextPort) {
            nextPort.postMessage({
              id: msg && msg.id,
              ok: false,
              error: String(err && err.message ? err.message : err),
            });
          }
        });
    });

    nextPort.onDisconnect.addListener(() => {
      void chrome.runtime.lastError;
      if (port === nextPort) {
        port = null;
      }
      scheduleReconnect();
    });
  } catch {
    port = null;
    scheduleReconnect();
  }
}

function ensureAlarm() {
  chrome.alarms.create(KEEPALIVE_ALARM, { periodInMinutes: 1 });
}

chrome.runtime.onStartup.addListener(() => {
  ensureAlarm();
  connect();
});

chrome.runtime.onInstalled.addListener(() => {
  ensureAlarm();
  connect();
});

chrome.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === KEEPALIVE_ALARM && !port) {
    connect();
  }
});

ensureAlarm();
connect();
