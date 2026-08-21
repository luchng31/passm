# GitHub 私有仓库及 Token 配置指南

本文档用于记录如何创建 GitHub 私有仓库并生成具有读写权限的 Fine-grained Personal Access Token。

## 1. 创建私有仓库

首先，在 GitHub 上创建一个新的私有仓库：

- 登录 GitHub，点击右上角 `+` 号，选择 **New repository**。
- 填写仓库名称，并选择 **Private**（私有）可见性。
- 点击 **Create repository** 完成创建，仓库即准备就绪。

## 2. 生成 Personal Access Token

为了安全地访问上述私有仓库，需要创建一个 Fine-grained Personal Access Token：

1. 访问 Token 创建页面：  
   👉 [https://github.com/settings/personal-access-tokens/new](https://github.com/settings/personal-access-tokens/new)

2. 填写 Token 基本信息：
   - **Token name**：建议填写为与你的仓库名一致，便于后续管理。
   - **Description**：可写可不写，用于备注该 Token 的用途。
   - **Expiration**：选择 **No expiration**（永不过期）。

3. 配置仓库访问权限：
   - 在 **Repository access** 选项中，选择 **Only select repositories**。
   - 在下方弹出的仓库列表中，选中你在第一步中创建的私有仓库。

4. 配置仓库权限（Permissions）：
   - 点击 **Add permission** 按钮。
   - 在搜索框中输入 `Contents` 并搜索。
   - 将 `Contents` 对应的 **access** 权限改为 **Read and write**（读写）。

5. 生成并保存 Token：
   - 滚动到页面底部，点击 **Generate token**。
   - **重要**：生成后请立即复制并妥善保存该 Token，页面关闭后将不再显示完整 Token。

## ⚠️ 注意事项

- 请勿将 Token 泄露给他人，也不要将其直接写在代码或公开文档中。
- 如果后续仓库名称或权限发生变化，请及时更新对应 Token 的设置。