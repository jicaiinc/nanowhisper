执行 NanoWhisper 发版流程（自动 patch 版本叠加，无需询问）：

1. 读取当前 package.json 中的 version，自动将 patch 版本号 +1（例如 0.1.7 → 0.1.8）
2. 修改 package.json 中的 version 字段
3. 执行 npm run sync-version 同步版本到 Cargo.toml 和 tauri.conf.json
4. 确认三个文件的版本号都已更新一致
5. 提交 git commit，message 格式：`[milestone] vX.Y.Z`
6. 创建 git tag：`vX.Y.Z`
7. push commit + tag 到远程
