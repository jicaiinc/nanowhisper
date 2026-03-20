执行 NanoWhisper 发版流程：

1. 读取当前 package.json 中的 version
2. 询问我新版本号是什么（提供 patch/minor/major 选项）
3. 修改 package.json 中的 version 字段为新版本号
4. 执行 npm run sync-version 同步版本到 Cargo.toml 和 tauri.conf.json
5. 确认三个文件的版本号都已更新一致
6. 提交 git commit，message 格式：`[milestone] vX.Y.Z`
7. 创建 git tag：`vX.Y.Z`
8. 询问我是否要 push 到远程（push commit + tag）
