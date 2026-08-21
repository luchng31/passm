# passm

本地优先、跨平台的密码管理器，通过**你自己的 Git 仓库**（如 GitHub 私有仓库）进行同步。基于 Tauri v2（Rust + WebView）与 React + TypeScript 构建。

## 特性

- **跨平台**：Android（APK）与 Linux 桌面（单一自包含二进制）
- **本地优先、端到端加密**：使用主密码解锁，密钥仅在本地派生
- **自托管同步**：通过你掌控的 Git 远端同步；Personal Access Token 仅保存在系统密钥库，绝不写入仓库
- **条目管理**：标题 / 用户名 / 密码 / 网址 / 备注
- **高效操作**：搜索、一键复制（密码 / 用户名 / 网址）、强密码生成器
- **冲突安全同步**：合并前自动备份，多设备修改自动收敛
- **体验细节**：自动锁定（计时 + 系统锁）、托盘图标、深色模式、安全区（刘海 / 底部手势）适配

## 工作原理（简述）

1. **首次使用**：将 passm 指向一个 Git 仓库并填入 PAT（见 [Create_pw_vault.md](./Create_pw_vault.md)）。
2. **创建保险库**：用主密码创建，主密码不存储，仅在本地派生密钥。
3. **添加条目**：内容加密后推送到你的仓库。
4. **多设备同步**：在另一台设备用相同主密码解锁，同步即拉取最新数据。

> 安全说明：主密码永不出设备；PAT 存于系统密钥库；保险库数据存放在**你的**仓库中，数据的控制权归你。

## 安装

- **Android**：侧载最新的 `passm-vX.Y.Z.apk`（同签名证书，可直接覆盖安装）。
- **Linux**：直接运行 `passm` 二进制（需系统已安装 `webkit2gtk` 运行库）。

## 从源码构建

环境要求：Node 24、Rust 工具链、Android SDK/NDK（构建 APK）、`webkit2gtk-4.1` 开发库（Linux 桌面）。

```bash
npm install

# 桌面端（自包含二进制，跳过联网打包）
npx tauri build --no-bundle

# Android（APK）
# 注意：tauri android build 默认不会把前端打进 APK 资源，
# 需要把 dist 注入未签名包后再 zipalign + 签名：
npx tauri android build --apk -t aarch64
# 随后将 dist 注入 APK、zipalign -p 4、使用 debug.keystore 签名
```

## 文档

- [Create_pw_vault.md](./Create_pw_vault.md)：配置私有仓库与 Fine-grained PAT 以启用同步。
