<p align="center">
  <img src="src-tauri/logo/appicon.png" alt="NanoWhisper" width="128" height="128">
</p>

<h1 align="center">NanoWhisper</h1>

<p align="center">
  <strong>纯粹的语音转文字，仅此而已。</strong>
</p>

<p align="center">
  <a href="https://github.com/jicaiinc/nanowhisper/releases/latest"><img alt="Latest Release" src="https://img.shields.io/github/v/release/jicaiinc/nanowhisper?style=flat-square&color=1c1c1e"></a>
  <a href="LICENSE"><img alt="License" src="https://img.shields.io/github/license/jicaiinc/nanowhisper?style=flat-square&color=1c1c1e&cacheSeconds=1"></a>
  <img alt="Platform" src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-333?style=flat-square">
</p>

<p align="center">
  <a href="https://github.com/jicaiinc/nanowhisper/releases/latest">下载</a>
</p>

<p align="center">
  <a href="README.md">English</a> | 简体中文
</p>

---

NanoWhisper 是一个极简的桌面语音转文字工具。按下快捷键，说话，转写的文字自动粘贴到你正在使用的应用中。就这么简单。

基于 OpenAI Whisper API，使用 Tauri v2 构建。

## 使用方式

1. 轻按 `右 ⌘` (macOS) / `右 Ctrl` (Windows)（可自定义）
2. 说话
3. 再按一次停止 — 文字瞬间转写并粘贴

## 特性

- **一个快捷键** — 全局热键启停录音，无需操作界面。
- **自动粘贴** — 转写文字直达光标位置，无需手动复制。
- **波形浮窗** — 录音时显示极简的置顶波形动画。
- **历史记录** — 所有转写结果本地保存，附带音频文件可重试。
- **系统托盘** — 安静地驻留后台。

## 从源码构建

前置条件：[Node.js](https://nodejs.org/) 和 [Rust](https://rustup.rs/)。

```bash
git clone https://github.com/jicaiinc/nanowhisper.git
cd nanowhisper
npm install
npm run tauri dev
```

## 许可证

[Apache License 2.0](LICENSE)

---

<p align="center">
  Pure Whisper. Nothing else.<br>
  <sub>&copy; 2025 <a href="https://github.com/jicaiinc">Jicai, Inc.</a></sub>
</p>
