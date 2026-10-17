# 参与贡献

感谢你对 Arca 的关注。提交改动前请先阅读本文，并留意 [LICENSE](LICENSE)：本项目是非商业许可，你提交的内容将按相同条款分发。

## 开发环境

- Rust：版本由 [`rust-toolchain.toml`](rust-toolchain.toml) 固定，`rustup` 会自动安装
- Node.js 与 npm：用于 `frontend/`
- Docker 与 make：`make dev` 会在缺少 PostgreSQL / Redis 时用 Docker 拉起它们

```bash
cp .env.example .env     # 设置 ADMIN_PASSWORD
make dev                 # 后端 + 前端
```

## 提交前检查

后端（与 CI 保持一致）：

```bash
cargo fmt --all --check
cargo clippy -p aether-gateway --lib --bins --examples -- -D warnings
cargo clippy -p aether-data --all-targets -- -D warnings
cargo test --locked -p aether-ai-formats --lib --tests    # 协议转换相关改动
```

涉及数据库的测试需要一个隔离的 PostgreSQL，并通过 `*_TEST_DATABASE_URL` 指定，不要指向有真实数据的库。

前端：

```bash
cd frontend
npm ci
npm run type-check
npm run test:run
npm run lint
```

## 提交规范

提交信息使用 [Conventional Commits](https://www.conventionalcommits.org/) 风格，例如：

```text
feat(routing): add strategy failover controls
fix(conversion): improve stream failures and diagnostic exports
docs: update architecture diagrams
```

常用类型：`feat`、`fix`、`refactor`、`docs`、`test`、`chore`。一个提交只做一件事，并说明动机。

## Pull Request

- 保持改动聚焦；无关的格式化或重命名请单独提交。
- 行为变更需要附带测试；协议转换改动请同时更新 [docs/api](docs/api/) 下对应的契约或矩阵文档。
- 不要提交真实的密钥、`.env`、数据库快照或生产配置。
- 文案与文档使用 Arca 名称；`aether-*` 的 crate、目录与二进制名属于继承自上游的标识，重命名会破坏已部署实例，请不要在无关 PR 中改动。

## 报告问题

提交 Issue 时请附上版本或 commit、部署方式、复现步骤，以及已脱敏的日志。涉及安全的问题请不要在公开 Issue 中披露细节，先通过维护者的 GitHub 主页联系。
