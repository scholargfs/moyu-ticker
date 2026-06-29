# A++ · 摸鱼盯盘浮窗

一个 macOS 上**透明置顶、可拖动的小浮窗**,实时显示自选 A 股涨跌,点开看当日分时图。为「上班摸鱼盯盘」而生:内存占用小、一眼可读、一键即隐,绝不弹窗打断。

---

## 核心特性

- **透明置顶浮窗**:无边框、透明背景、始终置顶、可拖动;鼠标移出自动变暗(约 16% 不透明度),移入恢复。
- **实时行情**:列表展示自选股 名称 / 现价 / 涨跌幅,**红涨绿跌**(A 股习惯)。交易时段每 3 秒刷新。
- **当日分时图**:点某只股 → ECharts 渲染当日分时(价格线 + 均价线 + 昨收基准线),配色与浮窗统一。
- **老板键**:全局快捷键 `⌥⌘H` 瞬间隐藏 / 唤出浮窗。
- **菜单栏托盘**:顶部状态栏单色模板图标(随深浅色自动反色),左键弹出菜单:显示/隐藏浮窗、刷新行情、退出。
- **红叉退出**:浮窗顶栏红色 `✕`,一键彻底退出整个进程。
- **自选股管理**:设置页增删代码,本地持久化,即改即生效。
- **隐蔽设计**:不进 Dock、不在 ⌘Tab 出现(`Accessory` 模式);任何异常静默降级,绝不弹窗。

---

## 技术栈

| 层 | 选型 |
|---|---|
| 框架 | [Tauri 2](https://tauri.app)(Rust 后端 + Web 前端,打包为 macOS `.app`) |
| 前端 | 原生 HTML / CSS / JS(不上框架,保持轻量) |
| 图表 | [ECharts](https://echarts.apache.org)(当日分时图) |
| 后端 | Rust(`reqwest` 抓取、`encoding_rs` 解码 GBK、`chrono` 时段判断、`serde` 序列化) |
| 持久化 | `tauri-plugin-store` |
| 快捷键 | `tauri-plugin-global-shortcut` |
| 数据源 | 腾讯免费行情接口(无需 API Key) |

---

## 数据源

均为腾讯公开接口,无需鉴权。请求时带 `Referer: https://finance.qq.com`。

- **实时报价**:`https://qt.gtimg.cn/q=<code1>,<code2>,...`
  - 返回 **GBK 编码**,需解码;每行形如 `v_sh600519="1~贵州茅台~600519~现价~昨收~..."`,按 `~` 切分。
  - 涨跌额 / 涨跌幅由「现价 − 昨收」自行计算,不依赖易变的字段下标。
- **当日分时**:`https://web.ifzq.gtimg.cn/appstock/app/minute/query?code=<code>`(UTF-8 JSON)
  - 每个分时点形如 `"0930 1199.00 969 116183100.00"` = 时间 / 价 / 累计量(手) / 累计额(元)。
  - 均价线 = 累计额 ÷ (累计量 × 100)。

**股票代码格式**:`sh` / `sz` 前缀 + 6 位代码,如 `sh000001`(上证指数)、`sz399001`(深证成指)、`sh600519`(贵州茅台)。默认自选即这三只。

---

## 架构

```
┌─ Rust 后端 (src-tauri/src) ───────────────────────────┐
│  quote.rs    实时行情:GBK 解码 + 按 ~ 解析 → Quote      │
│  minute.rs   当日分时:JSON 解析 → MinuteData           │
│  poll.rs     交易时段判断 + 后台轮询线程,emit 事件      │
│  lib.rs      应用接线:窗口策略 / 老板键 / 托盘 /         │
│              自选股持久化 / invoke 命令                  │
│  main.rs     入口,调用 lib::run()                       │
└────────────────────────────────────────────────────────┘
            │  invoke 命令 / event 事件
┌─ 前端 (src) ──────────────────────────────────────────┐
│  index.html  顶栏 + 列表 / 分时图 / 设置 三视图          │
│  main.js     视图切换、行情渲染、ECharts、事件监听        │
│  styles.css  深色主题、红涨绿跌、隐身变暗                 │
│  echarts.min.js                                         │
└────────────────────────────────────────────────────────┘
```

### 后端命令(invoke)

| 命令 | 作用 |
|---|---|
| `get_watchlist` | 读取自选股列表 |
| `set_watchlist(codes)` | 写入自选股(清洗 + 持久化) |
| `quotes_now` | 立即抓一次行情(不受交易时段限制,用于启动/手动刷新) |
| `fetch_minute(code)` | 抓某只股当日分时 |
| `quit_app` | 彻底退出应用(`app.exit(0)`) |

### 前端事件(listen)

| 事件 | 来源 | 作用 |
|---|---|---|
| `quotes-updated` | 轮询线程 | 盘中推送最新行情 |
| `quotes-stale` | 轮询线程 | 抓取失败,标记「未更新」灰点 |
| `tray-refresh` | 托盘菜单 | 触发前端刷新 |

### 轮询策略(`poll.rs`)

- **交易时段**(周一至五 09:30–11:30、13:00–15:00,本地时区):每 **3 秒**抓一次并 emit。
- **盘后 / 午休 / 周末**:每 60 秒只醒来判断是否开盘,**不抓数据**(省电、降暴露),界面保留最后一次快照。

---

## 数据流

1. 启动 → 从 store 读自选股(无则用默认)→ 前端渲染列表 → `quotes_now` 抓首屏。
2. `poll.rs` 后台轮询:盘中每 3s 抓取 → emit `quotes-updated` → 前端更新现价与涨跌色。
3. 点某只股 → `invoke fetch_minute(code)` → 切到分时视图,ECharts 渲染走势。
4. 设置页增删代码 → `invoke set_watchlist` → 写入 store,下一轮即生效。

---

## 开发与构建

### 环境要求

- [Rust](https://rustup.rs) + Cargo
- Node.js(用于 Tauri CLI)
- macOS(当前仅适配桌面 macOS;iOS/Android 图标已生成但未做适配)

### 安装依赖

```bash
npm install
```

### 开发运行(热重载)

```bash
npm run tauri dev
```

### 打包正式版

```bash
npm run tauri build
# 产物:src-tauri/target/release/bundle/macos/A++.app
```

### 安装到「应用程序」

```bash
cp -R src-tauri/target/release/bundle/macos/A++.app /Applications/
```

之后可在 启动台 / Spotlight 搜索 `A++` 打开。注意:运行时**不在 Dock 显示**(隐蔽设计),通过右上角托盘图标或浮窗操作。

> 一键关闭所有实例:`pkill -f "MacOS/A"`

---

## 测试

后端含 14 个单元测试(quote / minute / poll 三模块),覆盖:

- 行情 GBK 解析:正常行、多行、缺字段/非数字跳过、涨跌正负号。
- 分时 JSON 解析:正常解析 + 元数据、缺 code、非法 JSON、异常分时点跳过。
- 交易时段判断:早盘 / 午盘 / 午休 / 盘前 / 盘后 / 周末边界,3s↔60s 间隔切换。

```bash
cd src-tauri && cargo test
```

---

## 隐蔽性设计

- **老板键** `⌥⌘H`:全局瞬间隐藏/唤出。
- **鼠标移出变暗**:离开窗口降到约 16% 不透明度。
- **不进 Dock / ⌘Tab**:`ActivationPolicy::Accessory`。
- **托盘模板图标**:单色随菜单栏明暗反色,低调如系统自带。
- **绝不弹窗**:接口超时/失败保留上次数据并显示灰点;单只解析失败跳过该只;网络全断静默重试。

---

## 不做(YAGNI)

价格提醒 / 到价通知、下单 / 券商登录、历史 K 线 / 多日走势、多市场(美股/港股)。首版聚焦「轻量隐蔽地看自选股实时涨跌 + 当日分时」。

---

## 目录结构

```
A++/
├── src/                  前端
│   ├── index.html
│   ├── main.js
│   ├── styles.css
│   └── echarts.min.js
├── src-tauri/            Rust 后端
│   ├── src/
│   │   ├── lib.rs        应用接线(窗口/老板键/托盘/持久化/命令)
│   │   ├── main.rs       入口
│   │   ├── quote.rs      实时行情
│   │   ├── minute.rs     当日分时
│   │   └── poll.rs       时段判断 + 轮询
│   ├── icons/            应用图标 + 托盘模板图标(tray-template.png)
│   ├── capabilities/     权限配置
│   ├── Cargo.toml
│   └── tauri.conf.json   窗口 / 打包 / 命名配置
├── docs/                 设计文档
└── package.json
```

---

## 设计文档

详见 [`docs/superpowers/specs/2026-06-28-moyu-stock-ticker-design.md`](docs/superpowers/specs/2026-06-28-moyu-stock-ticker-design.md)。

---

## 免责声明

行情数据来自腾讯公开接口,仅供个人参考,可能存在延迟或错误,不构成任何投资建议。本工具不涉及任何交易下单功能。
