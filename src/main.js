const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const el = (id) => document.getElementById(id);
const fmt = (n, d = 2) => (n == null ? "—" : Number(n).toFixed(d));

// A股:涨红、跌绿、平灰
function trendClass(change) {
  if (change > 0) return "up";
  if (change < 0) return "down";
  return "flat";
}
function signed(n, d = 2) {
  const s = Number(n).toFixed(d);
  return n > 0 ? `+${s}` : s;
}

let order = []; // 自选股代码顺序
const byCode = new Map(); // code -> quote

// ---------- 列表 ----------
function renderList() {
  const ul = el("rows");
  el("list-empty").hidden = order.length > 0;
  ul.innerHTML = "";
  for (const code of order) {
    const q = byCode.get(code);
    const li = document.createElement("li");
    li.className = "row";
    li.dataset.code = code;
    if (!q) {
      li.innerHTML =
        `<div><div class="name">${code}</div><div class="code">${code}</div></div>` +
        `<div class="price">—</div><div class="pct flat">—</div>`;
    } else {
      const cls = trendClass(q.change);
      li.innerHTML =
        `<div><div class="name">${q.name}</div><div class="code">${q.code}</div></div>` +
        `<div class="price ${cls}">${fmt(q.price)}</div>` +
        `<div class="pct ${cls}">${signed(q.change_pct)}%</div>`;
    }
    li.addEventListener("click", () => openChart(code));
    ul.appendChild(li);
  }
}

function applyQuotes(quotes) {
  for (const q of quotes) byCode.set(q.code, q);
  renderList();
}

// ---------- 分时图 ----------
let chart = null;
let currentChartCode = null;

function showView(name) {
  for (const v of ["list", "chart", "settings"]) {
    el(`view-${v}`).hidden = v !== name;
  }
}

async function openChart(code) {
  currentChartCode = code;
  const q = byCode.get(code);
  el("chart-name").textContent = q ? q.name : code;
  el("chart-price").textContent = q ? fmt(q.price) : "";
  el("chart-price").className = "mono " + (q ? trendClass(q.change) : "");
  el("chart-error").hidden = true;
  showView("chart");
  await loadMinute(code);
}

async function loadMinute(code) {
  try {
    const data = await invoke("fetch_minute", { code });
    drawChart(data);
    el("chart-error").hidden = true;
  } catch (e) {
    el("chart-error").hidden = false;
    if (chart) chart.clear();
  }
}

function drawChart(data) {
  if (!chart) chart = echarts.init(el("chart"), null, { renderer: "canvas" });
  const times = data.points.map((p) => p.time);
  const prices = data.points.map((p) => p.price);
  const avgs = data.points.map((p) => p.avg);
  const base = data.prev_close ?? (prices.length ? prices[0] : 0);
  const last = prices.length ? prices[prices.length - 1] : base;
  // 收盘价相对昨收:红涨绿跌
  const upColor = "#f0454b";
  const downColor = "#2ea043";
  const lineColor = last >= base ? upColor : downColor;

  chart.setOption({
    animation: false,
    grid: { left: 6, right: 8, top: 10, bottom: 16 },
    tooltip: {
      trigger: "axis",
      backgroundColor: "rgba(22,26,33,0.95)",
      borderColor: "#232a33",
      textStyle: { color: "#c9d1d9", fontSize: 11 },
      formatter: (ps) => {
        const t = ps[0].axisValue;
        const price = ps.find((p) => p.seriesName === "价")?.data;
        const avg = ps.find((p) => p.seriesName === "均")?.data;
        const pct = base ? (((price - base) / base) * 100).toFixed(2) : "0";
        return `${t}<br/>价 ${fmt(price)} (${pct}%)<br/>均 ${fmt(avg)}`;
      },
    },
    xAxis: {
      type: "category",
      data: times,
      boundaryGap: false,
      axisLine: { lineStyle: { color: "#232a33" } },
      axisLabel: {
        color: "#6e7681",
        fontSize: 9,
        interval: Math.floor(times.length / 4) || 1,
      },
      axisTick: { show: false },
    },
    yAxis: {
      scale: true,
      position: "right",
      splitLine: { lineStyle: { color: "rgba(35,42,51,0.5)" } },
      axisLabel: { color: "#6e7681", fontSize: 9 },
    },
    series: [
      {
        name: "价",
        type: "line",
        data: prices,
        showSymbol: false,
        lineStyle: { color: lineColor, width: 1.2 },
        areaStyle: {
          color: {
            type: "linear", x: 0, y: 0, x2: 0, y2: 1,
            colorStops: [
              { offset: 0, color: lineColor + "44" },
              { offset: 1, color: lineColor + "05" },
            ],
          },
        },
        markLine: {
          symbol: "none",
          silent: true,
          data: [{ yAxis: base }],
          lineStyle: { color: "#6e7681", type: "dashed", width: 0.8 },
          label: { show: false },
        },
      },
      {
        name: "均",
        type: "line",
        data: avgs,
        showSymbol: false,
        lineStyle: { color: "#d8a657", width: 0.9 },
      },
    ],
  });
}

// ---------- 设置 ----------
async function openSettings() {
  const list = await invoke("get_watchlist");
  el("codes").value = list.join("\n");
  showView("settings");
}

async function saveSettings() {
  const codes = el("codes").value
    .split("\n")
    .map((s) => s.trim())
    .filter(Boolean);
  await invoke("set_watchlist", { codes });
  order = await invoke("get_watchlist");
  byCode.clear();
  renderList();
  showView("list");
  refresh();
}

// ---------- 刷新 / 状态点 ----------
let dotTimer = null;
function pulse(state) {
  const dot = el("dot");
  dot.className = state; // "live" | "stale"
  if (state === "live") {
    clearTimeout(dotTimer);
    dotTimer = setTimeout(() => (dot.className = ""), 1200);
  }
}

async function refresh() {
  try {
    const quotes = await invoke("quotes_now");
    applyQuotes(quotes);
    pulse("live");
  } catch (e) {
    pulse("stale");
  }
}

// ---------- 隐身:鼠标移出变暗 ----------
document.addEventListener("mouseleave", () => document.body.classList.add("dim"));
document.addEventListener("mouseenter", () => document.body.classList.remove("dim"));

// ---------- 接线 ----------
el("btn-refresh").addEventListener("click", refresh);
el("btn-settings").addEventListener("click", openSettings);
el("btn-quit").addEventListener("click", () => invoke("quit_app"));
el("btn-back").addEventListener("click", () => showView("list"));
el("btn-close-settings").addEventListener("click", () => showView("list"));
el("btn-save").addEventListener("click", saveSettings);
el("chart-retry").addEventListener("click", () => loadMinute(currentChartCode));
window.addEventListener("resize", () => chart && chart.resize());

listen("quotes-updated", (e) => {
  applyQuotes(e.payload);
  pulse("live");
});
listen("quotes-stale", () => pulse("stale"));
listen("tray-refresh", () => refresh());

(async function init() {
  el("bar-label").textContent = "MARKET";
  order = await invoke("get_watchlist");
  renderList();
  refresh();
})();
