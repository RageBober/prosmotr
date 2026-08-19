"use strict";

/* Интерфейс знает только формы документа: html, text, table, tree, page, image.
   Какие форматы за ними стоят — забота Rust. Поэтому этот файл не растёт,
   когда в ядро добавляют новый формат: растёт только список расширений там. */

const TAURI = window.__TAURI__;
const invoke = TAURI ? TAURI.core.invoke : null;
const HL = window.hljs && (window.hljs.default || window.hljs);

const S = {
  root: null,
  entries: [],
  tree: null,
  collapsed: new Set(), // свёрнутые папки
  query: "",
  tabs: [], // {path, entry, doc, mode, scroll, url}
  active: -1,
};

const $ = (id) => document.getElementById(id);
const view = $("view"), empty = $("empty"), scroller = $("scroller"), app = $("app");

function el(tag, cls, text) {
  const n = document.createElement(tag);
  if (cls) n.className = cls;
  if (text != null) n.textContent = text;
  return n;
}
const fmt = (n) => n.toLocaleString("ru-RU");
const size = (b) => (b < 1024 ? b + " Б" : b < 1048576 ? (b / 1024).toFixed(1) + " КБ" : (b / 1048576).toFixed(1) + " МБ");

function notice(title, body) {
  view.innerHTML = "";
  const box = el("div", "notice");
  box.appendChild(el("b", null, title));
  box.appendChild(document.createTextNode(body || ""));
  view.appendChild(box);
}

/* ---------------- папка ---------------- */

async function pick() {
  if (!invoke) return notice("Нет связи с ядром", "Интерфейс запущен вне приложения.");
  try {
    const root = await invoke("pick_folder");
    if (root) await loadRoot(root);
  } catch (e) {
    notice("Не удалось открыть папку", String(e));
  }
}

async function loadRoot(root, keepPath) {
  S.root = root;
  try {
    S.entries = await invoke("list_files", { root });
  } catch (e) {
    return notice("Не удалось прочитать папку", String(e));
  }
  $("folderName").textContent = root.split("/").filter(Boolean).pop() || root;
  $("folderName").title = root;

  // вкладки на исчезнувшие файлы закрываем, у оставшихся сбрасываем кеш
  const alive = new Set(S.entries.map((e) => e.path));
  S.tabs = S.tabs.filter((t) => alive.has(t.path));
  S.tabs.forEach((t) => {
    t.doc = null;
    t.entry = S.entries.find((e) => e.path === t.path) || t.entry;
  });
  if (S.active >= S.tabs.length) S.active = S.tabs.length - 1;

  renderTree();
  renderTabs();

  if (!S.entries.length) {
    empty.style.display = "block";
    view.innerHTML = "";
    empty.querySelector("h2").textContent = "В папке нет подходящих файлов";
    return;
  }
  const keep = keepPath && S.entries.find((e) => e.path === keepPath);
  if (keep) return openEntry(keep);
  if (S.tabs.length) return activate(Math.max(0, S.active));
  const first = S.entries.find((e) => /^(00|readme|index)/i.test(e.name));
  openEntry(first || S.entries[0]);
}

/* ---------------- дерево файлов ---------------- */

function visible() {
  const q = S.query.trim().toLowerCase();
  return q ? S.entries.filter((e) => e.rel.toLowerCase().includes(q)) : S.entries;
}

function buildTree(entries) {
  const root = { name: "", path: "", dirs: new Map(), files: [], count: 0 };
  for (const e of entries) {
    let node = root;
    node.count++;
    if (e.dir) {
      for (const part of e.dir.split("/")) {
        if (!node.dirs.has(part)) {
          node.dirs.set(part, {
            name: part,
            path: node.path ? node.path + "/" + part : part,
            dirs: new Map(),
            files: [],
            count: 0,
          });
        }
        node = node.dirs.get(part);
        node.count++;
      }
    }
    node.files.push(e);
  }
  return root;
}

const FILE_ICON =
  '<svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linejoin="round"><path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/></svg>';

function renderTree() {
  const box = $("fileTree");
  box.innerHTML = "";
  const items = visible();
  if (!items.length) {
    box.appendChild(el("div", "empty-list", S.entries.length ? "Ничего не найдено" : "Откройте папку"));
    return;
  }
  S.tree = buildTree(items);
  const filtering = S.query.trim().length > 0;
  walk(S.tree, box, 0, filtering);
}

function walk(node, parent, depth, filtering) {
  for (const dir of node.dirs.values()) {
    // при поиске показываем всё раскрытым, иначе — по памяти о сворачивании
    const open = filtering || !S.collapsed.has(dir.path);
    const row = el("div", "dir" + (open ? " open" : ""));
    row.style.paddingLeft = 8 + depth * 12 + "px";
    row.appendChild(el("span", "caret", "▸"));
    row.appendChild(el("span", "dname", dir.name));
    row.appendChild(el("span", "cnt", String(dir.count)));
    row.title = dir.path;
    row.onclick = () => {
      if (S.collapsed.has(dir.path)) S.collapsed.delete(dir.path);
      else S.collapsed.add(dir.path);
      renderTree();
    };
    parent.appendChild(row);

    const kids = el("div", "kids" + (open ? "" : " hidden"));
    parent.appendChild(kids);
    walk(dir, kids, depth + 1, filtering);
  }
  for (const e of node.files) {
    const cur = S.tabs[S.active];
    const row = el("div", "file" + (cur && cur.path === e.path ? " active" : ""));
    row.style.paddingLeft = 9 + depth * 12 + "px";
    row.innerHTML = FILE_ICON;
    row.appendChild(el("span", "fname", e.name));
    if (e.category !== "doc") row.appendChild(el("span", "tag", e.ext));
    row.title = e.rel + " · " + size(e.size);
    row.onclick = () => openEntry(e);
    parent.appendChild(row);
  }
}

/* ---------------- вкладки ---------------- */

function renderTabs() {
  const bar = $("tabs");
  bar.innerHTML = "";
  S.tabs.forEach((t, i) => {
    const tab = el("div", "tab" + (i === S.active ? " active" : ""));
    tab.appendChild(el("span", "tname", t.entry.name));
    const x = el("span", "x", "×");
    x.title = "Закрыть (Ctrl+W)";
    x.onclick = (ev) => { ev.stopPropagation(); closeTab(i); };
    tab.appendChild(x);
    tab.title = t.entry.rel;
    tab.onclick = () => activate(i);
    tab.onauxclick = (ev) => { if (ev.button === 1) { ev.preventDefault(); closeTab(i); } };
    bar.appendChild(tab);
  });
  const cur = bar.querySelector(".tab.active");
  if (cur) cur.scrollIntoView({ block: "nearest", inline: "nearest" });
}

function openEntry(entry, anchor) {
  let i = S.tabs.findIndex((t) => t.path === entry.path);
  if (i < 0) {
    S.tabs.push({ path: entry.path, entry, doc: null, mode: "render", scroll: 0, url: null });
    i = S.tabs.length - 1;
  }
  activate(i, anchor);
}

function closeTab(i) {
  const t = S.tabs[i];
  if (t && t.url) URL.revokeObjectURL(t.url);
  S.tabs.splice(i, 1);
  if (!S.tabs.length) {
    S.active = -1;
    view.innerHTML = "";
    empty.style.display = "block";
    $("docName").textContent = "Документ не выбран";
    $("docExt").textContent = "—";
    $("docMeta").textContent = "";
    renderTabs();
    renderTree();
    return;
  }
  activate(Math.min(i, S.tabs.length - 1));
}

function activate(i, anchor) {
  const prev = S.tabs[S.active];
  if (prev) prev.scroll = scroller.scrollTop;
  S.active = i;
  renderTabs();
  renderTree();
  show(anchor);
}

async function show(anchor) {
  const t = S.tabs[S.active];
  if (!t) return;
  empty.style.display = "none";
  $("docName").textContent = t.entry.name.replace(/\.[^.]+$/, "");
  $("docName").title = t.entry.rel;
  $("docExt").textContent = (t.entry.ext || "—").toUpperCase();
  $("docMeta").textContent = "";
  $("btnRender").classList.toggle("active", t.mode !== "src");
  $("btnSource").classList.toggle("active", t.mode === "src");
  document.title = t.entry.name + " — Просмотрщик";

  if (t.mode === "src") return showSource(t);

  if (!t.doc) {
    view.innerHTML = "";
    try {
      t.doc = await invoke("read_doc", { path: t.path });
    } catch (e) {
      return notice("Не удалось открыть файл", String(e));
    }
    if (S.tabs[S.active] !== t) return; // пока читали, ушли на другую вкладку
  }
  renderDoc(t, anchor);
}

/* ---------------- показ форм ---------------- */

function renderDoc(tab, anchor) {
  const doc = tab.doc;
  view.innerHTML = "";
  $("tocList").innerHTML = "";
  scroller.onscroll = null;
  if (doc.kind !== "html") {
    app.classList.add("no-toc");
    $("btnToc").classList.remove("active");
  }
  switch (doc.kind) {
    case "html": return renderHtml(doc, tab, anchor);
    case "text": return renderText(doc);
    case "table": return renderTable(doc);
    case "tree": return renderTree_(doc);
    case "image": return renderImage(doc, tab);
    case "page": return renderPdf(tab);
    default: return notice("Пока не поддерживается", doc.message || "");
  }
}

/* --- html --- */

function slug(s) {
  return s.toLowerCase().replace(/[^\wа-яё\s-]/gi, "").trim().replace(/\s+/g, "-").slice(0, 60) || "h";
}

function renderHtml(doc, tab, anchor) {
  const entry = tab.entry;
  const art = el("article", "md");
  art.innerHTML = doc.html;
  view.appendChild(art);

  const used = new Set();
  art.querySelectorAll("h1,h2,h3,h4").forEach((h) => {
    let id = slug(h.textContent), n = 2;
    while (used.has(id)) id = slug(h.textContent) + "-" + n++;
    used.add(id);
    h.id = id;
    const a = el("a", "anchor", "#");
    a.onclick = () => h.scrollIntoView({ block: "start" });
    h.appendChild(a);
  });

  art.querySelectorAll("a[href]").forEach((a) => {
    const href = a.getAttribute("href") || "";
    if (/^(https?:|mailto:|tel:)/i.test(href)) {
      a.onclick = (ev) => { ev.preventDefault(); if (TAURI && TAURI.opener) TAURI.opener.openUrl(href); };
      return;
    }
    if (href.startsWith("#")) {
      a.onclick = (ev) => {
        ev.preventDefault();
        const t = art.querySelector("#" + CSS.escape(decodeURIComponent(href.slice(1))));
        if (t) t.scrollIntoView({ block: "start" });
      };
      return;
    }
    const [file, frag] = decodeURIComponent(href).split("#");
    const target = resolve(entry.dir, file);
    const found = S.entries.find((e) => e.rel === target);
    if (found) {
      a.onclick = (ev) => { ev.preventDefault(); openEntry(found, frag ? slug(frag) : null); };
    } else {
      a.classList.add("dead");
      a.title = "В папке нет файла: " + target;
      a.onclick = (ev) => ev.preventDefault();
    }
  });

  if (HL) art.querySelectorAll("pre code").forEach((c) => { try { HL.highlightElement(c); } catch (e) {} });

  buildToc(art);
  if (anchor) {
    const t = art.querySelector("#" + CSS.escape(anchor));
    if (t) return t.scrollIntoView({ block: "start" });
  }
  scroller.scrollTop = tab.scroll || 0;
}

function resolve(dir, rel) {
  const stack = dir ? dir.split("/") : [];
  for (const part of rel.split("/")) {
    if (!part || part === ".") continue;
    if (part === "..") stack.pop();
    else stack.push(part);
  }
  return stack.join("/");
}

function buildToc(art) {
  const list = $("tocList");
  list.innerHTML = "";
  const hs = [...art.querySelectorAll("h1,h2,h3,h4")];
  if (hs.length < 2) { app.classList.add("no-toc"); $("btnToc").classList.remove("active"); return; }
  for (const h of hs) {
    const a = el("a", h.tagName === "H3" ? "h3" : h.tagName === "H4" ? "h4" : null, h.textContent.replace(/#$/, ""));
    a.href = "#" + h.id;
    a.onclick = (ev) => { ev.preventDefault(); h.scrollIntoView({ block: "start" }); };
    list.appendChild(a);
  }
}

/* --- text (с подсветкой) --- */

/** Разрезать подсвеченный html по строкам, не порвав теги. */
function splitHighlighted(html) {
  const lines = [];
  const stack = [];
  let cur = "";
  const re = /(<span[^>]*>)|(<\/span>)|([^<]+)/g;
  let m;
  while ((m = re.exec(html))) {
    if (m[1]) { stack.push(m[1]); cur += m[1]; }
    else if (m[2]) { stack.pop(); cur += m[2]; }
    else {
      const parts = m[3].split("\n");
      parts.forEach((p, i) => {
        if (i > 0) { cur += "</span>".repeat(stack.length); lines.push(cur); cur = stack.join(""); }
        cur += p;
      });
    }
  }
  lines.push(cur);
  return lines;
}

function renderText(doc) {
  const box = el("div", "text-view");
  const plain = doc.text.split("\n");
  let html = null;
  if (HL && doc.lang) {
    try {
      if (HL.getLanguage(doc.lang)) {
        html = splitHighlighted(HL.highlight(doc.text, { language: doc.lang, ignoreIllegals: true }).value);
        if (html.length !== plain.length) html = null; // подстраховка: рассинхрон строк
      }
    } catch (e) { html = null; }
  }

  const frag = document.createDocumentFragment();
  plain.forEach((line, i) => {
    const row = el("div", "ln");
    row.appendChild(el("span", "num", String(i + 1)));
    const src = el("span", "src");
    if (html) src.innerHTML = html[i] || " ";
    else src.textContent = line || " ";
    row.appendChild(src);
    frag.appendChild(row);
  });
  box.appendChild(frag);
  view.appendChild(box);
  $("docMeta").textContent =
    fmt(plain.length) + " строк" + (doc.lang ? " · " + doc.lang : "") + (doc.truncated ? " · обрезано" : "");
  scroller.scrollTop = 0;
}

/* --- table --- */

const isNum = (s) => /^-?\d[\d  ]*([.,]\d+)?$/.test(s.trim()) && s.trim() !== "";
const toNum = (s) => parseFloat(s.replace(/[  ]/g, "").replace(",", "."));

function renderTable(doc) {
  const CHUNK = 200;
  let sortCol = -1, sortDir = 1, filter = "", shown = 0;
  const rows = doc.rows;

  const wrap = el("div", "table-wrap");
  const bar = el("div", "table-bar");
  const input = el("input");
  input.type = "search";
  input.placeholder = "Фильтр по строкам…";
  const info = el("span", "info");
  bar.append(input, info);

  const table = el("table", "grid");
  const thead = el("thead");
  const htr = el("tr");
  doc.columns.forEach((c, i) => {
    const th = el("th", null, c || "—");
    const ord = el("span", "ord");
    th.appendChild(ord);
    th.onclick = () => {
      sortDir = sortCol === i ? -sortDir : 1;
      sortCol = i;
      [...htr.children].forEach((x) => (x.querySelector(".ord").textContent = ""));
      ord.textContent = sortDir > 0 ? "▲" : "▼";
      apply();
    };
    htr.appendChild(th);
  });
  thead.appendChild(htr);
  const tbody = el("tbody");
  table.append(thead, tbody);
  wrap.append(bar, table);
  const more = el("button", "btn more", "Показать ещё");
  wrap.appendChild(more);
  view.appendChild(wrap);

  let data = rows;
  function current() {
    let out = rows;
    if (filter) out = out.filter((r) => r.some((c) => c.toLowerCase().includes(filter)));
    if (sortCol >= 0) {
      out = out.slice().sort((a, b) => {
        const x = a[sortCol] ?? "", y = b[sortCol] ?? "";
        const both = isNum(x) && isNum(y);
        return (both ? toNum(x) - toNum(y) : x.localeCompare(y, "ru")) * sortDir;
      });
    }
    return out;
  }
  function addChunk() {
    const frag = document.createDocumentFragment();
    const end = Math.min(shown + CHUNK, data.length);
    for (let i = shown; i < end; i++) {
      const tr = el("tr");
      for (const cell of data[i]) tr.appendChild(el("td", isNum(cell) ? "num" : null, cell));
      frag.appendChild(tr);
    }
    tbody.appendChild(frag);
    shown = end;
    more.style.display = shown < data.length ? "" : "none";
    info.textContent = fmt(shown) + " из " + fmt(data.length) +
      (doc.truncated ? " (в файле больше, загружены первые " + fmt(rows.length) + ")" : "");
  }
  function apply() {
    data = current();
    tbody.innerHTML = "";
    shown = 0;
    addChunk();
  }

  more.onclick = addChunk;
  input.oninput = () => { filter = input.value.trim().toLowerCase(); apply(); };
  scroller.onscroll = () => {
    if (shown < data.length && scroller.scrollTop + scroller.clientHeight > scroller.scrollHeight - 300) addChunk();
  };

  apply();
  $("docMeta").textContent =
    fmt(doc.total_rows) + " строк · " + doc.columns.length + " столбцов · " + doc.delimiter;
  scroller.scrollTop = 0;
}

/* --- tree --- */

function renderTree_(doc) {
  const box = el("div", "tree");

  function leaf(key, value) {
    const row = el("div", "row");
    if (key !== null) row.appendChild(el("span", "k", key + ": "));
    let cls = "s", text = String(value);
    if (typeof value === "number") cls = "n";
    else if (typeof value === "boolean") cls = "b";
    else if (value === null) { cls = "nil"; text = "null"; }
    else text = '"' + value + '"';
    row.appendChild(el("span", cls, text));
    return row;
  }
  function node(value, key) {
    if (value === null || typeof value !== "object") return leaf(key, value);
    const arr = Array.isArray(value);
    const entries = arr ? value.map((v, i) => [String(i), v]) : Object.entries(value);
    const d = el("details");
    if (entries.length <= 40) d.open = true;
    const sum = el("summary");
    if (key !== null) sum.appendChild(el("span", "k", key + ": "));
    sum.appendChild(document.createTextNode(arr ? "[ … ]" : "{ … }"));
    sum.appendChild(el("span", "count", entries.length + (arr ? " элементов" : " полей")));
    d.appendChild(sum);
    for (const [k, v] of entries) d.appendChild(node(v, k));
    return d;
  }

  box.appendChild(node(doc.json, null));
  view.appendChild(box);
  scroller.scrollTop = 0;
}

/* --- image --- */

async function bytesOf(path) {
  const res = await invoke("read_bytes", { path });
  return res instanceof ArrayBuffer ? res : new Uint8Array(res).buffer;
}

async function renderImage(doc, tab) {
  const wrap = el("div", "img-wrap");
  const bar = el("div", "img-bar");
  const info = el("span", null, "загрузка…");
  const btnFit = el("button", "btn", "Вписать");
  const btnReal = el("button", "btn", "100%");
  bar.append(btnFit, btnReal, info);
  const stage = el("div", "img-stage");
  wrap.append(bar, stage);
  view.appendChild(wrap);

  try {
    const buf = await bytesOf(tab.path);
    if (tab.url) URL.revokeObjectURL(tab.url);
    tab.url = URL.createObjectURL(new Blob([buf], { type: doc.mime }));
    const img = el("img");
    img.src = tab.url;
    img.onload = () => {
      info.textContent = img.naturalWidth + " × " + img.naturalHeight + " · " + size(tab.entry.size) + " · " + doc.mime;
      $("docMeta").textContent = img.naturalWidth + " × " + img.naturalHeight;
    };
    img.onerror = () => (info.textContent = "не удалось показать картинку");
    stage.appendChild(img);
    btnFit.onclick = () => stage.classList.remove("real");
    btnReal.onclick = () => stage.classList.add("real");
  } catch (e) {
    notice("Не удалось прочитать картинку", String(e));
  }
}

/* --- page (pdf) --- */

let pdfLib = null;
async function ensurePdf() {
  if (!pdfLib) {
    pdfLib = await import("./vendor/pdf.min.mjs");
    pdfLib.GlobalWorkerOptions.workerSrc = "vendor/pdf.worker.min.mjs";
  }
  return pdfLib;
}

async function renderPdf(tab) {
  const wrap = el("div");
  const bar = el("div", "pdf-bar");
  const info = el("span", null, "открываю pdf…");
  const out = el("button", "btn", "−");
  const inn = el("button", "btn", "+");
  bar.append(out, inn, info);
  const pages = el("div", "pdf-pages");
  wrap.append(bar, pages);
  view.appendChild(wrap);

  let lib, pdf;
  try {
    lib = await ensurePdf();
    const buf = await bytesOf(tab.path);
    pdf = await lib.getDocument({ data: new Uint8Array(buf) }).promise;
  } catch (e) {
    return notice("Не удалось открыть pdf", String(e));
  }
  if (S.tabs[S.active] !== tab) return;

  let scale = 1.3;
  info.textContent = pdf.numPages + " страниц";
  $("docMeta").textContent = pdf.numPages + " страниц · " + size(tab.entry.size);

  // рисуем только то, что видно: сотня страниц разом положит окно
  let observer = null;
  async function layout() {
    pages.innerHTML = "";
    if (observer) observer.disconnect();
    const first = await pdf.getPage(1);
    const vp = first.getViewport({ scale });

    observer = new IntersectionObserver(
      (items) => {
        for (const it of items) {
          if (!it.isIntersecting) continue;
          const holder = it.target;
          if (holder.dataset.done) continue;
          holder.dataset.done = "1";
          draw(Number(holder.dataset.page), holder);
        }
      },
      { root: scroller, rootMargin: "600px 0px" }
    );

    for (let n = 1; n <= pdf.numPages; n++) {
      const holder = el("div", "pdf-ph", "страница " + n);
      holder.style.width = Math.round(vp.width) + "px";
      holder.style.height = Math.round(vp.height) + "px";
      holder.dataset.page = String(n);
      pages.appendChild(holder);
      observer.observe(holder);
    }
  }

  async function draw(n, holder) {
    try {
      const page = await pdf.getPage(n);
      const vp = page.getViewport({ scale });
      const canvas = el("canvas", "pdf-page");
      canvas.width = Math.round(vp.width);
      canvas.height = Math.round(vp.height);
      canvas.style.width = Math.round(vp.width) + "px";
      await page.render({ canvasContext: canvas.getContext("2d"), viewport: vp }).promise;
      holder.replaceWith(canvas);
    } catch (e) {
      holder.textContent = "страница " + n + " не нарисовалась";
    }
  }

  inn.onclick = () => { scale = Math.min(4, scale * 1.25); layout(); };
  out.onclick = () => { scale = Math.max(0.4, scale / 1.25); layout(); };
  await layout();
  scroller.scrollTop = tab.scroll || 0;
}

/* --- исходный текст --- */

async function showSource(tab) {
  view.innerHTML = "";
  try {
    const text = await invoke("read_source", { path: tab.path });
    renderText({ text, lang: tab.entry.ext, truncated: false });
  } catch (e) {
    notice("Не удалось прочитать файл", String(e));
  }
}

/* ---------------- кнопки и клавиатура ---------------- */

function setMode(m) {
  const t = S.tabs[S.active];
  if (!t) return;
  t.mode = m;
  show();
}

$("btnPick").onclick = pick;
$("btnPick2").onclick = pick;
$("btnRender").onclick = () => setMode("render");
$("btnSource").onclick = () => setMode("src");
$("btnRefresh").onclick = () => {
  const cur = S.tabs[S.active];
  return S.root ? loadRoot(S.root, cur && cur.path) : pick();
};
$("btnCollapseAll").onclick = function () {
  const all = new Set();
  (function collect(node) {
    for (const d of node.dirs.values()) { all.add(d.path); collect(d); }
  })(S.tree || buildTree(S.entries));
  // если уже всё свёрнуто — разворачиваем обратно
  const everything = [...all].every((p) => S.collapsed.has(p));
  S.collapsed = everything ? new Set() : all;
  this.textContent = everything ? "Свернуть" : "Развернуть";
  renderTree();
};
$("btnTabs").onclick = function () {
  app.classList.toggle("tabs-hidden");
  const hidden = app.classList.contains("tabs-hidden");
  this.classList.toggle("active", !hidden);
  this.title = hidden ? "Показать вкладки" : "Свернуть вкладки";
};
$("btnSide").onclick = function () {
  app.classList.toggle("no-side");
  this.classList.toggle("active", !app.classList.contains("no-side"));
};
$("btnToc").onclick = function () {
  app.classList.toggle("no-toc");
  this.classList.toggle("active", !app.classList.contains("no-toc"));
};
$("btnFull").onclick = () => {
  if (document.fullscreenElement) document.exitFullscreen();
  else document.documentElement.requestFullscreen().catch(() => {});
};
$("btnTheme").onclick = () => {
  const r = document.documentElement;
  r.dataset.theme = r.dataset.theme === "light" ? "dark" : "light";
};

let t = null;
$("search").addEventListener("input", (e) => {
  S.query = e.target.value;
  clearTimeout(t);
  t = setTimeout(renderTree, 120);
});

document.addEventListener("keydown", (e) => {
  const inField = /^(INPUT|TEXTAREA)$/.test(e.target.nodeName);
  const mod = e.ctrlKey || e.metaKey;

  if (mod && e.key.toLowerCase() === "k") { e.preventDefault(); $("search").focus(); $("search").select(); return; }
  if (mod && e.key.toLowerCase() === "w") { e.preventDefault(); if (S.active >= 0) closeTab(S.active); return; }
  if (mod && e.key === "Tab" && S.tabs.length > 1) {
    e.preventDefault();
    activate((S.active + (e.shiftKey ? -1 : 1) + S.tabs.length) % S.tabs.length);
    return;
  }
  if (mod && /^[1-9]$/.test(e.key)) {
    const i = Number(e.key) - 1;
    if (i < S.tabs.length) { e.preventDefault(); activate(i); }
    return;
  }
  if (e.key === "Escape" && inField) return e.target.blur();
  if (inField) return;

  if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    const list = visible();
    if (!list.length) return;
    e.preventDefault();
    const cur = S.tabs[S.active];
    let i = list.findIndex((x) => cur && x.path === cur.path);
    i = e.key === "ArrowDown" ? Math.min(list.length - 1, i + 1) : Math.max(0, i - 1);
    openEntry(list[i]);
  }
});

if (!invoke) {
  notice("Ядро недоступно", "Этот интерфейс работает внутри приложения prosmotr.");
} else {
  // папка или файл могли прийти из командной строки и из «Открыть с помощью»
  invoke("initial_folder")
    .then((s) => s && loadRoot(s.folder, s.file))
    .catch(() => {});
}
