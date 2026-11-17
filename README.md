# Checkout-R（jieshouyin）

离线优先、本地优先的收银桌面软件。技术栈：Rust + Tauri 2 + Vue 3 + SQLite（WAL）。目标平台 Windows 10/11 x64。

> 开发原则：不编造、不回避问题、不降级处理、不懂就搜索答案、不写冗余代码、处理问题不产生其他问题。

## 架构级硬约束

前台开单、支付、打印、挂单、退货、交班六项操作在**任何授权状态**下（包括年度授权到期未续费）均不被阻断。授权校验失败只影响后台管理、报表导出等非生存性功能。

代码中由以下机制保证：

- `LicenseStatus::allows_pos_core()` 除 `UNACTIVATED` 外一律放行（`crates/core/src/license/state.rs`）
- `LicenseSnapshot::gate()` 对 `pos.core` 在 GRACE_PERIOD / OFFLINE_DEGRADED 直接放行（`crates/core/src/license/service.rs`）
- Tauri `checkout` 命令仅调用 `gate("pos.core")`，不检查后台权限（`crates/app/src/commands.rs`）
- 授权状态机只在 Rust 领域模块实现，UI 只消费状态

## 仓库结构

```
crates/
  core/              核心领域库（纯 Rust，可独立测试）
    src/db/          迁移 0001–0006（完整 v1 schema + 种子）
    src/license/     授权：载荷/验签/指纹/状态机/审计/安全存储/启动序列
    src/rbac.rs      argon2id、门禁顺序、会话锁定、二次授权、限额
    src/promo.rs     促销引擎（取优/互斥/叠加/最大余数分摊/退单还原）
    src/member.rs    会员四账、不可变流水、镜像回补、券状态机
    src/inventory.rs 库存事件投影
    src/order.rs     订单状态机 tx A/B/C、挂单、退货、作废、交班
    src/payment.rs   支付渠道 trait、付款码/Native 状态机、退款路由、对账
    src/mock.rs      支付 Mock 网关（仅测试）
    src/escpos.rs    ESC/POS 渲染 + GB18030/位图回退 + 重打队列
    src/report.rs    日结/渠道分布/销售报表/差异 CSV
    src/scanner.rs   扫码楔入判定 + 称重店内码（EAN-13 MOD10）
    build.rs         内嵌公钥注入（CHECKOUT_PUBLIC_KEY）
  license-issuance/  独立签发 crate + CLI（私钥不进客户端）
  app/               Tauri 2 应用：命令层、严格 CSP、最小 capabilities
ui/                  Vue 3 + TypeScript + Vite + Pinia 前端
.github/workflows/   CI：fmt / clippy -D warnings / test / Windows x64 构建
```

## 已实现（对应文档任务）

| 模块 | 任务 | 说明 |
| --- | --- | --- |
| Migration | MIG-01~03 | 0001–0006 完整 schema、幂等迁移、表存在性测试 |
| 金额 | RPT-04 | 整数运算、cents_to_yuan 五用例、负数带符号 |
| 促销分摊 | PRO-01~04 | 最大余数法、三条不变式单元测试、退单精确还原 |
| 授权 | LIC-01~08/10/11 | Ed25519 验签、单调水位、状态机、审计、三级存储降级、门禁 |
| RBAC | RBAC-01/02 | argon2id、门禁五步顺序、60s 单次 nonce、限额 |
| 收银 | POS-01~06、PSK-01、VOD-01、RTN-01 | tx A/B/C、挂单、退货、作废、交班 |
| 会员 | MEM-01~04 | 四账流水、幂等键、镜像回补、券状态机 |
| 库存 | STK-01、PUR-01~04 | 事件投影、采购验收、盘点审批 |
| 支付 | PAY-00/07 | trait 抽象、付款码轮询/撤销、Native 轮询、Mock 网关 |
| 打印 | PRT-01~04/06 | ESC/POS 字节流、模板变量、中文编码、重打队列 |
| 报表 | RPT-01~03 | 日结幂等、渠道分布、销售多维 |
| 扫码 | SCN-01/02 | 楔入判定、称重码解析 |

## 实现边界（如实声明，不编造）

以下文档内容在本版本中**尚未实现**，代码未伪装其已完成：

1. **PAY-02~06**：微信/支付宝 APIv3 真实商户网关、回调验签（v2 中心形态）、对账 CSV 解析与差异分级写入。当前仅提供 trait 抽象、Mock 网关与差异类型定义；桌面端不开监听端口的约束已遵守。
2. **PRT-05**：网络连接打印机 IP:9100 发送字节流（当前只生成字节流，由硬件适配器发送）。
3. **LIC-09/12**：签发 CLI 已实现 init/issue/renew/rebind 命令，但未做安装包签名与硬件实测。
4. **A-14 keyring 降级**：实现了 keyring 与加密文件两条存储路径；运行时优先顺序的自动探测在 app 层使用加密文件（keyring 模块在 core 中可用，待接入）。
5. **组件库 O-01**：前端使用自绘组件与自绘 SVG 图标，未引入 Element Plus / Naive UI（文档标注为 M0 末实测决定）。
6. **性能基准 23.7**：10 万 SKU 压测、tx B ≤100ms、断电强杀测试未执行，需实体环境。
7. **GA 出口准则**：签名安装包、干净虚拟机端到端冒烟、全量验收清单 A-01~A-16 未执行。

## 构建

代码推送到 GitHub 后由 Actions 在 `windows-latest` 执行：

- `cargo fmt --all -- --check`
- `cargo clippy -p checkout-core -p license-issuance --all-targets -- -D warnings`
- `cargo test -p checkout-core -p license-issuance --all-targets`
- `cargo build --release -p checkout-app`（产物 `checkout-app.exe` 上传为 artifact）

## 签发工具

```bash
# 生成密钥对（私钥离线保管，勿提交仓库）
cargo run -p license-issuance -- init --product checkout-r --out-dir ./keys

# 签发（私钥通过环境变量或 --key 提供）
CHECKOUT_PRIVATE_KEY=<hex> cargo run -p license-issuance -- issue \
  --type ANNUAL --store-id STORE_001 --valid-until 2027-11-17 --grace-days 7 --out license.key
```

客户端构建注入公钥：设置环境变量 `CHECKOUT_PUBLIC_KEY=<64位hex>`。未提供时使用 RFC 8032 测试向量公钥（仅开发/编译用途，不可用于生产）。

## 安全基线

- Tauri CSP 严格模式：默认仅 `'self'`，禁止远程内容
- capabilities 仅 `core:default`，业务命令通过 `invoke_handler` 注册清单暴露
- 金额全程整数分运算，禁止浮点
- 许可证与金额路径禁止 `unwrap`/`expect`
- 商户密钥不落 SQLite、不进备份、不进日志
