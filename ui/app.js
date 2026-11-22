// Belfry UI: plain JS, talks to the Rust side through Tauri's invoke().
const invoke = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);
const $ = (sel, root = document) => root.querySelector(sel);

const settings = Object.assign(
  { nickname: "", lang: "auto", relay: "", closeAction: "ask", autoUpdate: true },
  JSON.parse(localStorage.getItem("belfry.settings") || "{}")
);
function saveSettings() {
  localStorage.setItem("belfry.settings", JSON.stringify(settings));
}

let status = null; // last Status from the service
let lastJson = "";
let problem = null; // { kind: "service" | "token", detail }
let openMenu = null; // room id whose menu is open
let installing = false;
let info = { version: "?", os: "linux" };
let trayOk = false;
let update = null; // { version, notes, can_install } when a newer release exists
let updating = null; // null | { done, total } while downloading
let serviceUpdating = false;
const listen = (ev, fn) => window.__TAURI__.event.listen(ev, fn);

const esc = (s) =>
  String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

function toast(msg, bad = false) {
  const el = $("#toast");
  el.textContent = msg;
  el.className = "show" + (bad ? " bad" : "");
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => (el.className = ""), bad ? 5000 : 1800);
}

function cleanErr(e) {
  return tErr(String(e).replace(/^(API|NO_SERVICE|NO_TOKEN|CANCELLED):\s*/, ""));
}

async function copy(text) {
  try {
    await invoke("plugin:clipboard-manager|write_text", { text });
  } catch {
    await navigator.clipboard.writeText(text);
  }
  toast(t("copied"));
}

// ---------- polling ----------

async function refresh() {
  try {
    const s = await invoke("get_status");
    problem = null;
    const json = JSON.stringify(s);
    if (json !== lastJson) {
      status = s;
      lastJson = json;
      render();
      syncTray();
    } else if (!$(".room-list") && !$(".empty")) {
      render();
    }
  } catch (e) {
    const msg = String(e);
    const kind = msg.startsWith("NO_TOKEN") ? "token" : msg.startsWith("NO_SERVICE") ? "service" : null;
    if (kind) {
      const changed = !problem || problem.kind !== kind;
      problem = { kind, detail: msg };
      status = null;
      lastJson = "";
      if (changed || !$(".setup")) render();
    }
  }
}

// ---------- rendering ----------

// Patch the live DOM to match new markup instead of replacing it: no flicker,
// no replayed animations, hover/focus survive the 2-second refresh.
function keyOf(n) {
  return n.nodeType === 1 ? n.getAttribute("data-key") : null;
}
function morph(from, to) {
  if (from.nodeType !== to.nodeType || from.nodeName !== to.nodeName) {
    from.replaceWith(to);
    return;
  }
  if (from.nodeType !== 1) {
    if (from.nodeValue !== to.nodeValue) from.nodeValue = to.nodeValue;
    return;
  }
  for (const a of [...from.attributes]) if (!to.hasAttribute(a.name)) from.removeAttribute(a.name);
  for (const a of [...to.attributes]) if (from.getAttribute(a.name) !== a.value) from.setAttribute(a.name, a.value);
  if (from.tagName === "INPUT" && from.type === "checkbox") from.checked = to.hasAttribute("checked");
  morphChildren(from, to);
}
function morphChildren(from, to) {
  const oldKeyed = new Map();
  for (const c of from.childNodes) if (keyOf(c)) oldKeyed.set(keyOf(c), c);
  let cursor = from.firstChild;
  for (const t of [...to.childNodes]) {
    const k = keyOf(t);
    let match = null;
    if (k && oldKeyed.has(k)) {
      match = oldKeyed.get(k);
      oldKeyed.delete(k);
    } else if (!k && cursor && !keyOf(cursor) && cursor.nodeName === t.nodeName) {
      match = cursor;
    }
    if (match) {
      if (match !== cursor) from.insertBefore(match, cursor);
      else cursor = cursor.nextSibling;
      morph(match, t);
    } else {
      from.insertBefore(t, cursor);
    }
  }
  while (cursor) {
    const next = cursor.nextSibling;
    cursor.remove();
    cursor = next;
  }
}
function setHTML(el, html) {
  const tpl = document.createElement(el.tagName);
  tpl.innerHTML = html;
  morphChildren(el, tpl);
}

function render() {
  const view = $("#view");
  $("#actions").hidden = !!problem || !status;
  if (problem) return renderSetup(view);
  renderBanner();
  if (!status) {
    setHTML(view, `<div class="loading"><div class="spinner"></div></div>`);
    return;
  }
  if (status.rooms.length === 0) {
    setHTML(view, `
      <section class="empty">
        <img src="bell.svg" alt="" class="hero-bell">
        <h1>${esc(t("emptyTitle"))}</h1>
        <p>${esc(t("emptyText"))}</p>
      </section>`);
    return;
  }
  const order = { online: 0, starting: 1, error: 2, off: 3 };
  const rooms = [...status.rooms].sort((a, b) => (order[a.state] - order[b.state]) || a.name.localeCompare(b.name));
  setHTML(view, `<section class="room-list">${rooms.map(roomCard).join("")}</section>`);
}

function stateLabel(s) {
  return { online: t("stOnline"), starting: t("stStarting"), off: t("stOff"), error: t("stError") }[s];
}

function latencyClass(ms) {
  if (ms == null) return "";
  if (ms < 60) return "good";
  if (ms < 140) return "ok";
  return "bad";
}

function memberRow(m) {
  const initial = esc((m.name || "?").trim().charAt(0).toUpperCase());
  const tags = [];
  if (m.is_host) tags.push(`<span class="tag host">${esc(t("host"))}</span>`);
  if (m.link === "me") tags.push(`<span class="tag me">${esc(t("you"))}</span>`);
  const right =
    m.link === "me"
      ? ""
      : `<span class="lat ${latencyClass(m.latency_ms)}">${m.latency_ms != null ? Math.round(m.latency_ms) + " ms" : "—"}</span>
         <span class="link ${m.link}" title="${esc(m.protocol)}">${esc(m.link === "direct" ? t("direct") : t("relayed"))}${
           ipFamily(m.protocol) ? ` · ${ipFamily(m.protocol)}` : ""
         }</span>`;
  return `
    <li class="member" data-key="${esc(m.ip)}">
      <span class="avatar" style="--h:${hue(m.name)}">${initial}</span>
      <span class="who"><span class="name">${esc(m.name)}</span>${tags.join("")}</span>
      <button class="ip mono" data-copy="${esc(m.ip)}" title="${esc(t("copyCode"))}">${esc(m.ip)}</button>
      ${right}
    </li>`;
}

// "tcp6,udp6" -> IPv6, "udp" -> IPv4: tells the host which path friends use.
function ipFamily(proto) {
  const p = String(proto || "").split(",").filter((x) => x && x !== "-");
  if (!p.length) return "";
  return p.some((x) => x.endsWith("6")) ? "IPv6" : "IPv4";
}

function hue(s) {
  let h = 0;
  for (const c of String(s)) h = (h * 31 + c.codePointAt(0)) % 360;
  return h;
}

function roomCard(r) {
  const isHost = r.role === "host";
  const others = r.members.filter((m) => m.link !== "me");
  let body = "";
  if (r.active) {
    const mainIp = r.host_ip;
    const mainLabel = isHost ? t("friendsUse") : t("hostIp");
    body += `
      <div class="ips">
        <div class="ipbox main">
          <span class="lbl">${esc(mainLabel)}</span>
          <button class="bigip mono" data-copy="${esc(mainIp)}">${esc(mainIp)}<svg viewBox="0 0 24 24"><path d="M16 1H4a2 2 0 0 0-2 2v14h2V3h12V1Zm3 4H8a2 2 0 0 0-2 2v14c0 1.1.9 2 2 2h11a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2Zm0 16H8V7h11v14Z"/></svg></button>
        </div>
        ${
          !isHost && r.my_ip
            ? `<div class="ipbox"><span class="lbl">${esc(t("yourIp"))}</span><button class="smallip mono" data-copy="${esc(r.my_ip)}">${esc(r.my_ip)}</button></div>`
            : ""
        }
      </div>`;
    if (r.error) body += `<div class="alert">${esc(tErr(r.error))}</div>`;
    if (r.members.length) {
      body += `<div class="section-label">${esc(t("members"))} · ${r.members.length}</div>
               <ul class="members">${r.members.map(memberRow).join("")}</ul>`;
    }
    if (r.state === "online" && isHost && others.length === 0) {
      body += `<p class="note">${esc(t("waitingFriends"))}</p>`;
    }
    if (r.state === "starting" && !isHost) body += `<p class="note pulse">${esc(t("lookingHost"))}</p>`;
    if (isHost) {
      body += `<div class="reach">${
        r.public_addrs.length
          ? `${esc(t("publicAddr"))} <span class="mono">${r.public_addrs.map(esc).join(" · ")}</span> · ${esc(t("port"))} <span class="mono">${r.port}</span>`
          : esc(t("noPublic"))
      }<span class="help" title="${esc(t("reachHelp", { port: r.port }))}">?</span></div>`;
    }
  }
  const menu =
    openMenu === r.id
      ? `<div class="menu" role="menu">
          ${isHost ? `<button data-act="code" data-id="${r.id}">${esc(t("menuCode"))}</button>` : ""}
          <button data-act="rename" data-id="${r.id}">${esc(t("menuName"))}</button>
          <button data-act="log" data-id="${r.id}">${esc(t("menuLog"))}</button>
          <button class="danger" data-act="delete" data-id="${r.id}">${esc(isHost ? t("menuDelete") : t("menuLeave"))}</button>
        </div>`
      : "";
  return `
    <article class="room ${r.state} ${r.active ? "" : "inactive"}" data-id="${r.id}" data-key="${r.id}">
      <div class="room-head">
        <span class="dot ${r.state}" title="${esc(stateLabel(r.state))}"></span>
        <div class="title">
          <h3>${esc(r.name)}</h3>
          <span class="sub"><span class="role ${r.role}">${esc(isHost ? t("host") : t("guest"))}</span> · ${esc(stateLabel(r.state))} · ${esc(r.nickname)}</span>
        </div>
        ${isHost ? `<button class="icon-btn small" data-act="code" data-id="${r.id}" title="${esc(t("copyCode"))}"><svg viewBox="0 0 24 24"><path d="M3.9 12a3.1 3.1 0 0 1 3.1-3.1h4V7H7a5 5 0 0 0 0 10h4v-1.9H7A3.1 3.1 0 0 1 3.9 12ZM8 13h8v-2H8v2Zm9-6h-4v1.9h4a3.1 3.1 0 0 1 0 6.2h-4V17h4a5 5 0 0 0 0-10Z"/></svg></button>` : ""}
        <label class="switch" title="${esc(r.active ? t("stOnline") : t("stOff"))}">
          <input type="checkbox" data-toggle="${r.id}" ${r.active ? "checked" : ""}>
          <span></span>
        </label>
        <button class="icon-btn small" data-act="menu" data-id="${r.id}" aria-label="menu"><svg viewBox="0 0 24 24"><path d="M12 8a2 2 0 1 0 0-4 2 2 0 0 0 0 4Zm0 2a2 2 0 1 0 0 4 2 2 0 0 0 0-4Zm0 6a2 2 0 1 0 0 4 2 2 0 0 0 0-4Z"/></svg></button>
        ${menu}
      </div>
      ${body}
    </article>`;
}

function renderSetup(view) {
  const linux = info.os === "linux";
  const isToken = problem.kind === "token";
  const title = isToken ? t("tokenTitle") : t("setupTitle");
  const text = isToken ? t("tokenText") : linux ? t("setupText") : t("setupWin");
  setHTML(view, `
    <section class="setup">
      <img src="bell.svg" alt="" class="hero-bell dim">
      <h1>${esc(title)}</h1>
      <p>${esc(text)}</p>
      <button class="btn primary big" id="btn-install" ${installing ? "disabled" : ""}>${esc(installing ? t("setupWorking") : t("setupBtn"))}</button>
      <details class="detail"><summary>${esc(t("advanced"))}</summary><code>${esc(problem.detail)}</code></details>
    </section>`);
  $("#btn-install").onclick = installService;
}

async function installService() {
  installing = true;
  render();
  try {
    await invoke("install_service");
    problem = null;
    toast("✓ Belfry");
  } catch (e) {
    if (!String(e).startsWith("CANCELLED")) toast(cleanErr(e), true);
  }
  installing = false;
  await refresh();
  render();
}


// ---------- banner: app update / outdated service ----------

function renderBanner() {
  const el = $("#banner");
  let html = "";
  if (status && info.version !== "?" && status.version !== info.version && !problem) {
    html = `<div class="banner" data-key="svc">
      <span>${esc(t("serviceOutdated", { svc: status.version, app: info.version }))}</span>
      <button class="btn small primary" data-act="svc-update" ${serviceUpdating ? "disabled" : ""}>${esc(
        serviceUpdating ? t("setupWorking") : t("updateService")
      )}</button></div>`;
  } else if (update) {
    const pct = updating && updating.total ? Math.round((100 * updating.done) / updating.total) : null;
    html = `<div class="banner" data-key="upd">
      <span>${esc(t("updateAvailable", { v: update.version }))}</span>
      ${
        updating
          ? `<span class="progress"><span style="width:${pct ?? 30}%"></span></span>`
          : `<button class="btn small primary" data-act="update">${esc(update.can_install ? t("updateNow") : t("download"))}</button>`
      }</div>`;
  }
  setHTML(el, html);
}

async function checkUpdate(manual = false) {
  try {
    update = await invoke("check_update");
    if (manual) toast(update ? t("updateAvailable", { v: update.version }) : t("upToDate"));
  } catch (e) {
    if (manual) toast(`${t("updateFailed")}: ${e}`, true);
  }
  renderBanner();
}

async function installUpdate() {
  if (!update) return;
  if (!update.can_install) {
    invoke("plugin:opener|open_url", { url: "https://github.com/ludoviciphilip/belfry/releases/latest" }).catch(() => {});
    return;
  }
  updating = { done: 0, total: 0 };
  renderBanner();
  try {
    await invoke("install_update"); // the app restarts when this succeeds
  } catch (e) {
    updating = null;
    toast(`${t("updateFailed")}: ${e}`, true);
    renderBanner();
  }
}

// ---------- tray & closing ----------

function syncTray() {
  if (!trayOk) return;
  const rooms = status ? status.rooms.filter((r) => r.state === "online") : [];
  const players = rooms.reduce((n, r) => n + r.members.length, 0);
  const tooltip = rooms.length ? `Belfry · ${t("trayOnline", { n: rooms.length, p: players })}` : "Belfry";
  const key = [LANG, tooltip].join("|");
  if (key === syncTray.last) return;
  syncTray.last = key;
  invoke("update_tray", { open: t("trayOpen"), quit: t("trayQuit"), tooltip }).catch(() => {});
}

function applyCloseAction() {
  invoke("set_close_action", { action: settings.closeAction }).catch(() => {});
}

// ---------- actions ----------

function openDialog(id) {
  const d = $(id);
  applyI18n(d);
  d.showModal();
  const first = d.querySelector("input, textarea");
  if (first) setTimeout(() => first.focus(), 30);
  return d;
}

function showShare(room) {
  $("#share-code").textContent = room.code;
  $("#share-addrs").textContent = room.public_addrs.length
    ? `${t("publicAddr")}: ${room.public_addrs.join(" · ")} · ${t("port")} ${room.port}`
    : t("noPublic");
  $("#btn-copy-share").onclick = () => copy(room.code);
  openDialog("#dlg-share");
}

function confirmBox(title, text, yesLabel) {
  return new Promise((resolve) => {
    const d = $("#dlg-confirm");
    $("#confirm-title").textContent = title;
    $("#confirm-text").textContent = text;
    $("#confirm-yes").textContent = yesLabel;
    d.returnValue = "";
    d.onclose = () => resolve(d.returnValue === "yes");
    openDialog("#dlg-confirm");
  });
}

function findRoom(id) {
  return status && status.rooms.find((r) => r.id === id);
}

document.addEventListener("click", async (ev) => {
  const copyEl = ev.target.closest("[data-copy]");
  if (copyEl) return copy(copyEl.dataset.copy);

  const closeEl = ev.target.closest("[data-close]");
  if (closeEl) return closeEl.closest("dialog").close();

  const act = ev.target.closest("[data-act]");
  if (!act) {
    if (openMenu && !ev.target.closest(".menu")) {
      openMenu = null;
      render();
    }
    return;
  }
  const id = act.dataset.id;
  const room = findRoom(id);
  switch (act.dataset.act) {
    case "update":
      installUpdate();
      break;
    case "svc-update":
      serviceUpdating = true;
      renderBanner();
      await installService();
      serviceUpdating = false;
      renderBanner();
      break;
    case "menu":
      openMenu = openMenu === id ? null : id;
      render();
      break;
    case "code":
      openMenu = null;
      render();
      if (room) showShare(room);
      break;
    case "rename": {
      openMenu = null;
      render();
      const f = $("#form-rename");
      f.nickname.value = room ? room.nickname : settings.nickname;
      f.dataset.id = id;
      openDialog("#dlg-rename");
      break;
    }
    case "log": {
      openMenu = null;
      render();
      try {
        $("#log-text").textContent = (await invoke("room_log", { id })) || "—";
      } catch (e) {
        $("#log-text").textContent = String(e);
      }
      openDialog("#dlg-log");
      const pre = $("#log-text");
      pre.scrollTop = pre.scrollHeight;
      break;
    }
    case "delete": {
      openMenu = null;
      render();
      if (!room) return;
      const host = room.role === "host";
      const ok = await confirmBox(
        t(host ? "confirmDelete" : "confirmLeave", { name: room.name }),
        t(host ? "confirmDeleteText" : "confirmLeaveText"),
        t(host ? "delete" : "leave")
      );
      if (ok) {
        try {
          await invoke("delete_room", { id });
        } catch (e) {
          toast(cleanErr(e), true);
        }
        refresh();
      }
      break;
    }
  }
});

document.addEventListener("change", async (ev) => {
  const tg = ev.target.closest("[data-toggle]");
  if (!tg) return;
  try {
    await invoke("set_room_active", { id: tg.dataset.toggle, active: tg.checked });
  } catch (e) {
    toast(cleanErr(e), true);
  }
  refresh();
});

$("#btn-create").onclick = () => {
  const f = $("#form-create");
  f.reset();
  f.nickname.value = settings.nickname;
  openDialog("#dlg-create");
};

$("#form-create").addEventListener("submit", async (ev) => {
  ev.preventDefault();
  const f = ev.target;
  const nickname = f.nickname.value.trim();
  settings.nickname = nickname;
  saveSettings();
  try {
    const room = await invoke("create_room", {
      name: f.name.value.trim(),
      nickname,
      relay: settings.relay.trim() || null,
    });
    $("#dlg-create").close();
    await refresh();
    showShare(room);
  } catch (e) {
    toast(cleanErr(e), true);
  }
});

$("#btn-join").onclick = async () => {
  const f = $("#form-join");
  f.reset();
  f.nickname.value = settings.nickname;
  $("#join-preview").hidden = true;
  openDialog("#dlg-join");
  // Offer what's on the clipboard if it looks like a code.
  try {
    const clip = await invoke("plugin:clipboard-manager|read_text");
    if (clip && /^\s*BLF/i.test(clip)) {
      f.code.value = clip.trim();
      previewJoin();
    }
  } catch {}
};

async function previewJoin() {
  const f = $("#form-join");
  const box = $("#join-preview");
  const code = f.code.value.trim();
  if (!code) return (box.hidden = true);
  try {
    const p = await invoke("preview_code", { code });
    box.className = "preview ok";
    box.innerHTML = `<b>${esc(t("previewRoom", { name: p.name }))}</b><br><span class="mono">${esc(
      t("previewVia", { addrs: p.addrs.join(" · ") })
    )}</span>`;
  } catch (e) {
    box.className = "preview bad";
    box.textContent = tErr(String(e));
  }
  box.hidden = false;
}
$("#form-join").code.addEventListener("input", previewJoin);

$("#form-join").addEventListener("submit", async (ev) => {
  ev.preventDefault();
  const f = ev.target;
  const nickname = f.nickname.value.trim();
  settings.nickname = nickname;
  saveSettings();
  try {
    await invoke("join_room", { code: f.code.value.trim(), nickname });
    $("#dlg-join").close();
    refresh();
  } catch (e) {
    toast(cleanErr(e), true);
  }
});

$("#form-rename").addEventListener("submit", async (ev) => {
  ev.preventDefault();
  const f = ev.target;
  const nickname = f.nickname.value.trim();
  if (!nickname) return;
  try {
    await invoke("set_nickname", { id: f.dataset.id, nickname });
    $("#dlg-rename").close();
  } catch (e) {
    toast(cleanErr(e), true);
  }
  refresh();
});

$("#btn-settings").onclick = () => {
  const f = $("#form-settings");
  f.nickname.value = settings.nickname;
  f.lang.value = settings.lang;
  f.relay.value = settings.relay;
  f.closeAction.value = settings.closeAction;
  f.autoUpdate.checked = settings.autoUpdate;
  $("#close-setting").hidden = !trayOk;
  $("#service-info").innerHTML = `
    <div><span>${esc(t("app"))}</span><span class="mono">${esc(info.version)}</span></div>
    <div><span>${esc(t("service"))}</span><span class="mono">${esc(status ? status.version : "—")}</span></div>
    <div><span>${esc(t("engine"))}</span><span class="mono">${esc(status ? status.engine_version : "—")}</span></div>`;
  openDialog("#dlg-settings");
};

$("#form-settings").addEventListener("submit", (ev) => {
  ev.preventDefault();
  const f = ev.target;
  settings.nickname = f.nickname.value.trim();
  settings.lang = f.lang.value;
  settings.relay = f.relay.value.trim();
  settings.closeAction = f.closeAction.value;
  settings.autoUpdate = f.autoUpdate.checked;
  saveSettings();
  applyCloseAction();
  LANG = pickLang(settings.lang);
  applyI18n();
  syncTray();
  $("#dlg-settings").close();
  render();
});

$("#btn-reinstall").onclick = () => {
  $("#dlg-settings").close();
  installService();
};

$("#btn-check-update").onclick = () => checkUpdate(true);

// X button with a tray: ask once, then remember (Settings can change it).
listen("close-requested", () => {
  const f = $("#form-close");
  f.remember.checked = true;
  openDialog("#dlg-close");
});
$("#dlg-close").addEventListener("click", async (ev) => {
  const b = ev.target.closest("[data-close-action]");
  if (!b) return;
  const action = b.dataset.closeAction;
  if ($("#form-close").remember.checked) {
    settings.closeAction = action;
    saveSettings();
    applyCloseAction();
  }
  $("#dlg-close").close();
  await invoke(action === "tray" ? "hide_window" : "quit_app");
});

listen("update-progress", (ev) => {
  const [done, total] = ev.payload;
  updating = { done, total: total || 0 };
  renderBanner();
});

$("#link-repo").onclick = (ev) => {
  ev.preventDefault();
  invoke("plugin:opener|open_url", { url: "https://github.com/ludoviciphilip/belfry" }).catch(() => {});
};

// ---------- start ----------

(async function start() {
  LANG = pickLang(settings.lang);
  applyI18n();
  try {
    info = await invoke("app_info");
    trayOk = await invoke("tray_available");
  } catch {}
  applyCloseAction();
  render();
  await refresh();
  syncTray();
  setInterval(refresh, 2000);
  if (settings.autoUpdate) {
    setTimeout(checkUpdate, 4000);
    setInterval(checkUpdate, 6 * 3600 * 1000);
  }
})();
