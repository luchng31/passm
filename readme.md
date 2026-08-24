# passm

本地优先、跨平台的密码管理器，通过**你自己的 Git 仓库**（如 GitHub 私有仓库）进行同步。基于 Tauri v2（Rust + WebView）与 React + TypeScript 构建。

## 特性

- **跨平台**：Windows（MSI / 绿色单文件）、Linux 桌面（自包含二进制）、Android（APK）
- **本地优先、端到端加密**：使用主密码解锁，密钥仅在本地派生
- **自托管同步**：通过你掌控的 Git 远端同步；Personal Access Token 仅保存在系统密钥库，绝不写入仓库
- **智能代理跟随**：同步自动走系统代理（Windows 系统代理 / `HTTPS_PROXY` 环境变量 / Android 系统代理注入），换代理地址无需任何配置
- **双栏工作台界面**：左侧紧凑条目列表、右侧只读详情面板，密码默认打码（眼睛图标切换明文），每个字段独立复制
- **瑞士风视觉**：暗色优先的高对比设计系统——细线分隔、微圆角、单一蓝色强调色；浅色模式跟随系统自动切换
- **条目管理**：标题 / 用户名 / 密码 / 网址 / 备注，搜索、强密码生成器、密码强度指示
- **冲突安全同步**：合并前自动备份，多设备修改自动收敛
- **体验细节**：自动锁定（计时 + 系统锁）、托盘图标、安全区（刘海 / 底部手势）适配

## 工作原理（简述）

1. **首次使用**：将 passm 指向一个 Git 仓库并填入 PAT（见 [Create_pw_vault.md](./Create_pw_vault.md)）。
2. **创建保险库**：用主密码创建，主密码不存储，仅在本地派生密钥。
3. **添加条目**：内容加密后推送到你的仓库。
4. **多设备同步**：在另一台设备用相同主密码解锁，同步即拉取最新数据。

> 安全说明：主密码永不出设备；PAT 存于系统密钥库；保险库数据存放在**你的**仓库中，数据的控制权归你。

## 安装

- **Windows**：运行 `passm_2.0.0_x64_en-US.msi` 安装；或直接运行绿色版 `passm-app.exe`（Win10/11 自带 WebView2，无需额外依赖）。
- **Android**：侧载最新的 `passm-vX.Y.Z.apk`（同签名证书，可直接覆盖安装）。
- **Linux**：直接运行 `passm` 二进制（需系统已安装 `webkit2gtk` 运行库）。

### 同步与代理

| 场景 | 行为 |
| --- | --- |
| Windows | 自动读取"设置 → 网络和 Internet → 代理"中的系统代理 |
| 显式覆盖 | 设置 `HTTPS_PROXY` 环境变量优先级更高（仅支持 `http://`，不支持 SOCKS） |
| Android | App 启动时由系统侧注入代理（见 `android_proxy.rs`） |
| 排查 | 运行 `cargo run -p passm-sync --example proxy_check` 查看当前生效的代理 |

## 从源码构建

环境要求：Node ≥ 20、Rust 工具链（stable）、Windows 需 MSVC 构建工具、Linux 桌面需 `webkit2gtk-4.1` 开发库、Android 需 SDK/NDK（构建 APK）。

```bash
npm install

# Windows 安装包（MSI）
npx tauri build --bundles msi

# Linux 桌面端（自包含二进制，跳过联网打包）
npx tauri build --no-bundle

# Android（APK）
# 注意：tauri android build 默认不会把前端打进 APK 资源，
# 需要把 dist 注入未签名包后再 zipalign + 签名：
npx tauri android build --apk -t aarch64
# 随后将 dist 注入 APK、zipalign -p 4、使用 debug.keystore 签名
```

## 开发与测试

```bash
npx tauri dev          # 桌面开发（前端热更新）
npm test               # 前端单元测试（vitest）
npm run build          # 前端类型检查 + 构建
cargo test --workspace # Rust 全量测试
cargo clippy --workspace --all-targets -- -D warnings
```

代码结构：

```
src/                  前端（React + TS，单文件设计系统 styles.css）
src-tauri/            Tauri 应用层（命令、托盘、自动锁定）
crates/passm-crypto   加密原语（Argon2 + ChaCha20-Poly1305）
crates/passm-vault    保险库数据模型与版本合并
crates/passm-sync     Git 同步、PAT 存储、代理解析
crates/passm-cli      命令行加解密工具
```

## 文档

- [Create_pw_vault.md](./Create_pw_vault.md)：配置私有仓库与 Fine-grained PAT 以启用同步。
