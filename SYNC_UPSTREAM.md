# safe-switch 同步上游手册

> 上游：https://github.com/farion1231/cc-switch
> 我们：https://github.com/BigDuLittle/safe-switch
> 基线：v4.0.1，本次同步到 v4.0.11（216 commits）

## 一、fork 改动地图（哪些是我们的，merge 时要保）

| 文件/目录 | 改动类型 | 同步策略 |
|---|---|---|
| `src-tauri/src/desensitize/` | 纯新增模块（10+ 文件） | 永远不冲突，自动合并 |
| `src-tauri/resources/models/bge-small-zh-v1.5/` | 纯新增（91MB 模型） | 不冲突 |
| `src-tauri/src/proxy/handlers.rs` | 文件末尾新增 `additive_chat_routes()` + macro | 上游改 handler 主体不影响，只在末尾追加 |
| `src-tauri/src/proxy/server.rs` | build_router 里一行 `.merge(handlers::additive_chat_routes())` | 冲突点 1 行 |
| `src-tauri/src/services/provider/live.rs` | write_live_snapshot 开头一行 `let provider = prepare_provider_for_additive(...)` | 冲突点 1 行 |
| `src-tauri/src/services/provider/pi.rs` | insert 前 3 行 rewrite | 独立文件，冲突少 |
| `src-tauri/src/app_config.rs` | `supports_local_proxy()` 返回值 | 偶发冲突 |
| `src-tauri/tauri.conf.json` | version + bundle.resources 模型目录 | 每次都冲突（version） |
| `src-tauri/Cargo.toml` | version + candle/tokenizers 依赖 | version 每次冲突 |
| `package.json` | version | version 每次冲突 |
| `.github/workflows/release.yml` | NSIS + macOS 判空跳过签名 | 偶发冲突 |
| `src/App.tsx` | 隐私保护路由 + ShieldCheck 图标 | 偶发冲突 |
| `src/pages/PrivacyProtection*` | 纯新增页面 | 不冲突 |

## 二、标准同步流程（每次上游发版跑一遍）

```powershell
cd C:\Users\admin\Desktop\tuomin\cc-switch

# 1. 拉上游新 tag
git fetch upstream --tags

# 2. 合并到 main
git merge vX.Y.Z --no-edit

# 3. 解决冲突（按下面"冲突清单"顺序）
# 4. 验证
cargo check
cargo fmt
# 5. 提交并推送
git add -A
git commit --no-edit
git push origin main
```

## 三、冲突清单（按本次实际结果）

### 必冲突（每次都出现）

**1. version 号（3 个文件）**

| 文件 |  ours（fork 版本） | theirs（上游版本） | 解法 |
|---|---|---|---|
| `package.json` | `0.1.x` | `4.0.x` | 取 ours，patch 号 +1（如 0.1.2 → 0.1.3） |
| `src-tauri/Cargo.toml` | `0.1.x` | `4.0.x` | 同上 |
| `src-tauri/tauri.conf.json` | `0.1.x` | `4.0.x` | 同上 |

> 注意：不要跟上游版本号对齐。我们是 fork，版本号独立递增。

**2. Cargo.lock**
直接 `git checkout --theirs src-tauri/Cargo.lock`，然后跑 `cargo check` 让它自动重新解析我们加的 candle/tokenizers 依赖。

### 偶发冲突（看上游改了什么）

**3. `src-tauri/src/desensitize/mod.rs` 里调用的 Database API**
上游可能重命名 dao 方法。本次就撞了：`get_proxy_listen_sync()` → `get_proxy_client_endpoint_sync()`。
解法：在 `src-tauri/src/database/dao/proxy.rs` 里 grep 新名字，改调用处。

**4. `src-tauri/src/proxy/handlers.rs`**
上游如果在文件末尾加新 handler，可能跟我们的 `additive_chat_routes()` 位置撞。
解法： ours 保留 `additive_chat_routes()` 函数，theirs 保留新 handler，两边都要。

**5. `src-tauri/src/proxy/server.rs`**
上游改 build_router 时可能动到我们的 `.merge()` 那行。
解法： ours 的 `.merge(handlers::additive_chat_routes())` 必须保留，其他按 theirs。

**6. `src-tauri/src/services/provider/live.rs`**
上游改 write_live_snapshot 时可能动到开头。
解法： ours 的 `let provider = prepare_provider_for_additive(...)` 必须保留。

**7. `src/App.tsx`**
上游加新图标 import 时可能撞我们的 `ShieldCheck`。
解法：两边 import 都保留。

**8. `.github/workflows/release.yml`**
上游改 release 流程时可能撞我们的 NSIS 配置和 macOS 判空。
解法：我们的 Windows target 必须是 NSIS（`bundle.windows.targets` 里 `["nsis"]`），macOS 签名用 bash 层判空。

## 四、已验证的死路（不要再踩）

- ❌ `get_proxy_listen_sync()` 已被上游改名为 `get_proxy_client_endpoint_sync()`
- ❌ WiX light.exe 处理 91MB safetensors 模型文件 exit 1（必须 NSIS）
- ❌ YAML step-level `if: ${{ secrets.APPLE_CERTIFICATE != '' }}` 会让 Actions 解析失败
- ❌ `APPLE_SIGNING_IDENTITY=`（空字符串）写入 GITHUB_ENV 会让 codesign 失败
- ❌ tauri.conf.json resources 用 glob `"src/models/*": "models/*"` 失败，必须目录映射 `"src/models/xxx": "models/xxx"`

## 五、本次同步结果（v4.0.1 → v4.0.11）

- 冲突文件：5 个（package.json / Cargo.lock / Cargo.toml / tauri.conf.json / App.tsx）
- 编译错误：1 个（get_proxy_listen_sync 改名）
- 手动修改点：0 个（desensitize 模块、路由、hook 全自动合并）
- 耗时：~15 分钟

## 六、后续同步建议

1. **上游发版就合**，不要攒。攒得越久冲突越多。
2. **每次同步后立刻发版**（patch 号 +1），让 CI 跑一遍 release 流水线。
3. **如果上游大改 proxy 或 live.rs**，先看 diff 再决定 merge 还是 cherry-pick 关键 fix。
4. **不要 force push**，正常 merge commit 就行，保留上游历史。
